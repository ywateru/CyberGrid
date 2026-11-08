#!/usr/bin/env python3
"""Private debug-test Lab controls. No retries, shell interpolation or token input."""
import argparse
import json
import os
import re
import subprocess
import sys
import time
import uuid

PACKAGE = "tech.tinadev.cybergrid.lab"
ROOT = "files/adb-control/"
MAX_REQUEST = 8192
MAX_RESULT = 1024 * 1024
SAFE_INT = 9007199254740991
# Shell source is constant; user JSON travels exclusively over stdin.
PUBLISH = ('set -eu; umask 077; mkdir files/adb-control/submit-lock; '
           'trap "rmdir files/adb-control/submit-lock" EXIT; '
           'test ! -e files/adb-control/request.json; '
           'cat > files/adb-control/request.tmp; '
           'mv files/adb-control/request.tmp files/adb-control/request.json')

# Missing files are an expected first-boot/first-result condition. A fixed
# frame distinguishes an existing empty/malformed file from an absent file.
READ_PREFIX = b"CYBERGRID_CONTROL_JSON\n"
READ_ERROR = b"CYBERGRID_CONTROL_READ_ERROR\n"
READ_COMMANDS = {
    "status.json": (
        'if [ -e files/adb-control/status.json ] || [ -L files/adb-control/status.json ]; then '
        'if [ -f files/adb-control/status.json ] && [ -r files/adb-control/status.json ] && [ ! -L files/adb-control/status.json ]; then '
        'printf "%s\\n" CYBERGRID_CONTROL_JSON; cat files/adb-control/status.json; '
        'else printf "%s\\n" CYBERGRID_CONTROL_READ_ERROR; fi; fi'
    ),
    "result.json": (
        'if [ -e files/adb-control/result.json ] || [ -L files/adb-control/result.json ]; then '
        'if [ -f files/adb-control/result.json ] && [ -r files/adb-control/result.json ] && [ ! -L files/adb-control/result.json ]; then '
        'printf "%s\\n" CYBERGRID_CONTROL_JSON; cat files/adb-control/result.json; '
        'else printf "%s\\n" CYBERGRID_CONTROL_READ_ERROR; fi; fi'
    ),
}

class Failure(Exception):
    def __init__(self, code, message, exit_code=2):
        super().__init__(message)
        self.code, self.exit_code = code, exit_code

class Adb:
    def __init__(self, serial):
        self.prefix = ["adb", "-s", serial]

    def call(self, args, data=None, timeout=10, max_bytes=MAX_RESULT):
        # Temporary spool prevents unbounded RAM even if the device misbehaves.
        import tempfile
        with tempfile.TemporaryDirectory(prefix="cybergrid-adb-") as directory:
            os.chmod(directory, 0o700)
            path = os.path.join(directory, "stdout")
            with open(path, "w+b") as output:
                os.chmod(path, 0o600)
                try:
                    result = subprocess.run(self.prefix + args, input=data, stdout=output,
                                            stderr=subprocess.DEVNULL, timeout=timeout, check=False)
                except subprocess.TimeoutExpired:
                    raise Failure("ADB_TIMEOUT", "ADB timed out; request is never automatically resent", 4)
                except OSError:
                    raise Failure("ADB_UNAVAILABLE", "Unable to run adb", 2)
                output.seek(0)
                value = output.read(max_bytes + 1)
                if len(value) > max_bytes:
                    raise Failure("RESPONSE_TOO_LARGE", "ADB response exceeded limit", 2)
                if result.returncode:
                    raise Failure("ADB_ERROR", "ADB operation failed (debug test app required)", 2)
                return value

    def live_pid(self):
        try:
            body = self.call(["shell", "pidof", PACKAGE], max_bytes=128)
        except Failure as error:
            if error.code == "ADB_ERROR":
                return None
            raise
        try:
            text = body.decode("ascii").strip()
            return int(text) if re.fullmatch(r"[0-9]{1,10}", text) else None
        except UnicodeError:
            return None

    def read(self, name):
        if name not in ("status.json", "result.json"):
            raise Failure("INVALID_FILE", "Only fixed control files are allowed")
        body = self.call(["shell", "-T", "run-as", PACKAGE, "sh", "-c", "'" + READ_COMMANDS[name] + "'"],
                         max_bytes=MAX_RESULT + len(READ_PREFIX))
        if not body:
            return None
        if body == READ_ERROR:
            raise Failure("CONTROL_READ_ERROR", "Control file is not a readable regular file")
        if not body.startswith(READ_PREFIX):
            raise Failure("INVALID_RESPONSE", "Invalid control JSON framing")
        body = body[len(READ_PREFIX):]
        if len(body) > MAX_RESULT:
            raise Failure("RESPONSE_TOO_LARGE", "Control JSON exceeded limit")
        try:
            value = json.loads(body)
            if not isinstance(value, dict):
                raise ValueError()
            return value
        except (ValueError, UnicodeError):
            raise Failure("INVALID_RESPONSE", "Invalid control JSON")

    def publish(self, request):
        data = json.dumps(request, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
        if len(data) > MAX_REQUEST:
            raise Failure("REQUEST_TOO_LARGE", "Request exceeds 8 KiB")
        self.call(["shell", "-T", "run-as", PACKAGE, "sh", "-c", "'" + PUBLISH + "'"], data, max_bytes=1024)


def metadata(value):
    if not isinstance(value, dict) or value.get("version") != 1:
        raise Failure("NOT_READY", "App control metadata unavailable")
    epoch, pid, high = value.get("controlEpoch"), value.get("pid"), value.get("highwater")
    if not isinstance(epoch, str) or not re.fullmatch("[0-9a-f]{32}", epoch):
        raise Failure("INVALID_RESPONSE", "Invalid epoch")
    if type(pid) is not int or pid <= 0 or type(high) is not int or not 0 <= high <= SAFE_INT:
        raise Failure("INVALID_RESPONSE", "Invalid process/sequence metadata")
    return epoch, pid, high


def wait_for_result(adb, request, pid, timeout):
    deadline = time.monotonic() + timeout
    while True:
        # A successful close can exit before a new status exists: result wins.
        result = adb.read("result.json")
        if result and all(result.get(k) == request[k] for k in ("requestId", "sequence", "controlEpoch")):
            if result.get("ok") is True:
                return result
            code = result.get("code", "APP_ERROR")
            raise Failure(code, "App rejected or failed the operation", 5 if code == "OUTCOME_UNKNOWN" else 3)
        if adb.live_pid() != pid:
            raise Failure("OUTCOME_UNKNOWN", "App exited or restarted; original request will not be replayed", 5)
        status = adb.read("status.json")
        if status:
            epoch, current_pid, _ = metadata(status)
            if epoch != request["controlEpoch"] or current_pid != pid:
                raise Failure("OUTCOME_UNKNOWN", "App restarted; original request will not be replayed", 5)
        if time.monotonic() >= deadline:
            raise Failure("OUTCOME_UNKNOWN", "No matching result before timeout; query status, do not replay", 5)
        time.sleep(0.2)


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--serial", required=True)
    p.add_argument("--timeout", type=float, default=90)
    sub = p.add_subparsers(dest="action", required=True)
    for action in ("start", "status", "cancel-login", "refresh", "disconnect", "logout", "close"):
        sub.add_parser(action)
    for action in ("login", "resume"):
        q = sub.add_parser(action)
        q.add_argument("--server", required=True)
        q.add_argument("--app", required=True)
        q.add_argument("--name", required=True)
        q.add_argument("--force-fallback", action="store_true")
    for action in ("connect", "probe"):
        sub.add_parser(action).add_argument("--device-id", required=True)
    sub.add_parser("send").add_argument("--text", required=True)
    return p


def main(argv=None, adb_factory=Adb):
    args = parser().parse_args(argv)
    if not re.fullmatch(r"[A-Za-z0-9._:-]{1,128}", args.serial) or not 0 < args.timeout <= 300:
        raise Failure("INVALID_ARGUMENT", "Invalid serial or timeout")
    adb = adb_factory(args.serial)
    if args.action == "start":
        adb.call(["shell", "am", "start", "-n", PACKAGE + "/.MainActivity"], max_bytes=8192)
        deadline = time.monotonic() + args.timeout
        while time.monotonic() < deadline:
            status = adb.read("status.json")
            if status and metadata(status)[1] == adb.live_pid():
                return status
            time.sleep(0.2)
        raise Failure("NOT_READY", "App did not publish metadata", 4)
    status = adb.read("status.json")
    epoch, pid, high = metadata(status)
    if adb.live_pid() != pid:
        raise Failure("NOT_RUNNING", "App is not running with current control metadata", 4)
    if args.action == "status":
        return status
    if high == SAFE_INT:
        raise Failure("SEQUENCE_EXHAUSTED", "Close and restart the App before another operation")
    payload = {}
    if args.action in ("login", "resume"):
        payload = {"server": args.server, "app": args.app, "name": args.name, "forceFallback": args.force_fallback}
    elif args.action in ("connect", "probe"):
        payload = {"deviceId": args.device_id}
    elif args.action == "send":
        payload = {"text": args.text}
    request = {"version": 1, "controlEpoch": epoch, "sequence": high + 1,
               "requestId": str(uuid.uuid4()), "action": args.action, "payload": payload}
    try:
        adb.publish(request)
    except Failure as error:
        if error.code == "REQUEST_TOO_LARGE":
            raise
        raise Failure("OUTCOME_UNKNOWN", "Submission transport failed; it may have been accepted, do not replay", 5)
    return wait_for_result(adb, request, pid, args.timeout)

if __name__ == "__main__":
    try:
        print(json.dumps(main(), ensure_ascii=False, indent=2))
    except Failure as failure:
        print(json.dumps({"ok": False, "code": failure.code, "message": str(failure)}))
        sys.exit(failure.exit_code)
