"""Host protocol checks with a fake ADB endpoint; never contacts a device."""
import importlib.util
import pathlib
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("android_adb", pathlib.Path(__file__).with_name("android-adb.py"))
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)

class FakeAdb:
    def __init__(self, serial):
        self.serial = serial
        self.sent = []
        self.status = {"version": 1, "controlEpoch": "a" * 32, "pid": 123, "highwater": 0}
        self.result = None
        self.pid = 123
        self.finish = True
        self.restart = False

    def live_pid(self):
        return self.pid

    def call(self, args, **kwargs):
        return b""

    def read(self, name):
        return self.result if name == "result.json" else self.status

    def publish(self, request):
        self.sent.append(request)
        if self.finish:
            self.result = {**request, "ok": True, "code": "OK"}
        if self.restart:
            self.pid += 1
            self.status = {**self.status, "pid": self.pid, "controlEpoch": "b" * 32}

class ProtocolTests(unittest.TestCase):
    def test_login_alias_uses_browser_wire_and_resume_keeps_optional_public_preset(self):
        self.assertIn("Alias for web-login", control.parser().format_help())
        for action in ("login", "web-login", "resume"):
            adb = FakeAdb("fixture")
            result = control.main(["--serial", "fixture", action], lambda _: adb)
            self.assertTrue(result["ok"])
            self.assertEqual(adb.sent[0]["action"], "web-login" if action == "login" else action)
            self.assertEqual(adb.sent[0]["payload"], {"forceFallback": False})
            self.assertEqual(len(adb.sent), 1)

    def test_web_login_only_accepts_public_configuration(self):
        adb = FakeAdb("fixture")
        control.main(["--serial", "fixture", "web-login", "--server", "https://example.test", "--app", "native-app", "--name", "Android", "--force-fallback"], lambda _: adb)
        self.assertEqual(adb.sent[0]["payload"], {"server": "https://example.test", "app": "native-app", "name": "Android", "forceFallback": True})
        for option in ("--token", "--refresh-token", "--code", "--redirect-uri", "--account"):
            with patch("sys.stderr"), self.assertRaises(SystemExit):
                control.main(["--serial", "fixture", "web-login", option, "forbidden"], lambda _: adb)
        self.assertEqual(len(adb.sent), 1)

    def test_real_command_envelope_and_config_whitelist(self):
        adb = FakeAdb("fixture")
        response = control.main(["--serial", "fixture", "login", "--server", "https://example.test", "--app", "id", "--name", "Android 4G"], lambda _: adb)
        request = adb.sent[0]
        self.assertEqual(request["action"], "web-login")
        self.assertEqual(response["sequence"], 1)
        self.assertEqual(set(request["payload"]), {"server", "app", "name", "forceFallback"})
        self.assertRegex(request["requestId"], r"^[a-f0-9-]{36}$")
        self.assertEqual(len(adb.sent), 1)

    def test_close_result_wins_over_process_exit(self):
        adb = FakeAdb("fixture")
        adb.restart = True
        result = control.main(["--serial", "fixture", "close"], lambda _: adb)
        self.assertTrue(result["ok"])

    def test_restart_unknown_never_retags_or_replays(self):
        adb = FakeAdb("fixture")
        adb.finish, adb.restart = False, True
        with self.assertRaises(control.Failure) as raised:
            control.main(["--serial", "fixture", "refresh"], lambda _: adb)
        self.assertEqual(raised.exception.code, "OUTCOME_UNKNOWN")
        self.assertEqual(len(adb.sent), 1)
        self.assertEqual(adb.sent[0]["controlEpoch"], "a" * 32)

    def test_timeout_unknown_and_no_resend(self):
        adb = FakeAdb("fixture")
        adb.finish = False
        with patch.object(control.time, "monotonic", side_effect=[0, 2]):
            with self.assertRaises(control.Failure) as raised:
                control.main(["--serial", "fixture", "--timeout", "1", "send", "--text", "message"], lambda _: adb)
        self.assertEqual(raised.exception.code, "OUTCOME_UNKNOWN")
        self.assertEqual(len(adb.sent), 1)

    def test_stale_status_does_not_permit_action(self):
        adb = FakeAdb("fixture")
        adb.pid = None
        with self.assertRaises(control.Failure) as raised:
            control.main(["--serial", "fixture", "refresh"], lambda _: adb)
        self.assertEqual(raised.exception.code, "NOT_RUNNING")
        self.assertEqual(adb.sent, [])

    def test_shell_and_paths_are_constant_stdin_transport(self):
        adb = control.Adb("fixture")
        with patch.object(adb, "call") as call:
            request = {"text": "$(touch should-not-run) ' ; & \n"}
            adb.publish(request)
        args, kwargs = call.call_args
        self.assertNotIn("should-not-run", str(args[0]))
        self.assertIn(b"should-not-run", args[1])
        self.assertEqual(args[0][0:5], ["shell", "-T", "run-as", control.PACKAGE, "sh"])
        with patch.object(adb, "call") as call:
            with self.assertRaises(control.Failure):
                adb.publish({"text": "x" * 8192})
            call.assert_not_called()

    def test_subprocess_argv_stdin_and_bounded_result(self):
        import subprocess
        from types import SimpleNamespace
        adb = control.Adb("fixture")
        def fake_run(argv, input, stdout, stderr, timeout, check):
            self.assertEqual(argv[:3], ["adb", "-s", "fixture"])
            self.assertEqual(input, b"payload")
            self.assertIs(stderr, subprocess.DEVNULL)
            stdout.write(b"{}")
            return SimpleNamespace(returncode=0)
        with patch.object(control.subprocess, "run", side_effect=fake_run):
            self.assertEqual(adb.call(["exec-out", "run-as", control.PACKAGE, "cat", "files/adb-control/status.json"], b"payload"), b"{}")
        def oversized(argv, **kwargs):
            kwargs["stdout"].write(control.READ_PREFIX + b"x" * (control.MAX_RESULT + 1))
            return SimpleNamespace(returncode=0)
        with patch.object(control.subprocess, "run", side_effect=oversized):
            with self.assertRaises(control.Failure) as raised:
                adb.read("status.json")
            self.assertEqual(raised.exception.code, "RESPONSE_TOO_LARGE")
        with self.assertRaises(control.Failure):
            adb.read("../../private-grant.json")

    def test_subprocess_timeout_does_not_retry_and_hides_stderr(self):
        import subprocess
        adb = control.Adb("fixture")
        with patch.object(control.subprocess, "run", side_effect=subprocess.TimeoutExpired(["adb"], 1, stderr=b"private-secret")) as call:
            with self.assertRaises(control.Failure) as raised:
                adb.publish({"safe": True})
            self.assertEqual(raised.exception.code, "ADB_TIMEOUT")
            self.assertNotIn("private-secret", str(raised.exception))
            self.assertEqual(call.call_count, 1)

    def test_submission_transport_error_has_unknown_outcome(self):
        adb = FakeAdb("fixture")
        def publish(request):
            adb.sent.append(request)
            raise control.Failure("ADB_TIMEOUT", "transport timeout", 4)
        adb.publish = publish
        with self.assertRaises(control.Failure) as raised:
            control.main(["--serial", "fixture", "send", "--text", "message"], lambda _: adb)
        self.assertEqual(raised.exception.code, "OUTCOME_UNKNOWN")
        self.assertEqual(raised.exception.exit_code, 5)
        self.assertEqual(len(adb.sent), 1)

    def test_read_absent_is_none_but_existing_invalid_json_is_rejected(self):
        adb = control.Adb("fixture")
        with patch.object(adb, "call", return_value=b""):
            self.assertIsNone(adb.read("status.json"))
            self.assertIsNone(adb.read("result.json"))
        for body in [control.READ_PREFIX, control.READ_PREFIX + b"broken", control.READ_PREFIX + b"[]", b"cat: No such file or directory\n", b"run-as: permission denied\n"]:
            with patch.object(adb, "call", return_value=body):
                with self.assertRaises(control.Failure) as raised:
                    adb.read("status.json")
                self.assertEqual(raised.exception.code, "INVALID_RESPONSE")
        for code in ["ADB_ERROR", "ADB_TIMEOUT", "ADB_UNAVAILABLE"]:
            with patch.object(adb, "call", side_effect=control.Failure(code, "failed")):
                with self.assertRaises(control.Failure) as raised:
                    adb.read("status.json")
                self.assertEqual(raised.exception.code, code)
        with patch.object(adb, "call", return_value=control.READ_ERROR):
            with self.assertRaises(control.Failure) as raised:
                adb.read("result.json")
            self.assertEqual(raised.exception.code, "CONTROL_READ_ERROR")

    def test_read_frame_preserves_full_one_mib_json_limit(self):
        import json
        adb = control.Adb("fixture")
        base = len(json.dumps({"data": ""}).encode())
        body = json.dumps({"data": "x" * (control.MAX_RESULT - base)}).encode()
        self.assertEqual(len(body), control.MAX_RESULT)
        with patch.object(adb, "call", return_value=control.READ_PREFIX + body) as call:
            self.assertEqual(len(adb.read("result.json")["data"]), control.MAX_RESULT - base)
            self.assertEqual(call.call_args.kwargs["max_bytes"], control.MAX_RESULT + len(control.READ_PREFIX))
        with patch.object(adb, "call", return_value=control.READ_PREFIX + body + b" "):
            with self.assertRaises(control.Failure) as raised:
                adb.read("result.json")
            self.assertEqual(raised.exception.code, "RESPONSE_TOO_LARGE")

    def test_fixed_read_shell_handles_missing_existing_empty_and_nonregular(self):
        import subprocess
        import tempfile
        import json
        with tempfile.TemporaryDirectory(prefix="cybergrid-adb-read-test-") as directory:
            root = pathlib.Path(directory)
            target = root / "files/adb-control"
            target.mkdir(parents=True)
            def local_call(args, **kwargs):
                self.assertEqual(args[:6], ["shell", "-T", "run-as", control.PACKAGE, "sh", "-c"])
                # Only the constant remote shell source executes in this sandbox.
                script = args[6][1:-1]
                self.assertIn(script, control.READ_COMMANDS.values())
                result = subprocess.run(["sh", "-c", script], cwd=root, capture_output=True, check=False, timeout=2)
                self.assertEqual(result.returncode, 0)
                return result.stdout
            adb = control.Adb("fixture")
            with patch.object(adb, "call", side_effect=local_call):
                for name in ["status.json", "result.json"]:
                    self.assertIsNone(adb.read(name))
                    path = target / name
                    path.write_text('{"version":1}')
                    self.assertEqual(adb.read(name), {"version": 1})
                    path.write_bytes(b"")
                    with self.assertRaises(control.Failure) as raised:
                        adb.read(name)
                    self.assertEqual(raised.exception.code, "INVALID_RESPONSE")
                    path.unlink()
                    path.mkdir()
                    with self.assertRaises(control.Failure) as raised:
                        adb.read(name)
                    self.assertEqual(raised.exception.code, "CONTROL_READ_ERROR")
                    path.rmdir()
                    path.symlink_to("nonexistent-control-target")
                    with self.assertRaises(control.Failure) as raised:
                        adb.read(name)
                    self.assertEqual(raised.exception.code, "CONTROL_READ_ERROR")
                    path.unlink()
                    import os
                    os.mkfifo(path)
                    with self.assertRaises(control.Failure) as raised:
                        adb.read(name)
                    self.assertEqual(raised.exception.code, "CONTROL_READ_ERROR")
                    path.unlink()

    def test_first_start_waits_for_missing_metadata_without_publishing(self):
        adb = FakeAdb("fixture")
        with patch.object(adb, "read", side_effect=[None, None, adb.status]), patch.object(control.time, "sleep"):
            self.assertEqual(control.main(["--serial", "fixture", "start"], lambda _: adb), adb.status)
        self.assertEqual(adb.sent, [])

    def test_first_result_waits_without_republishing_effect(self):
        adb = FakeAdb("fixture")
        original_read = adb.read
        missing = [True, True]
        def read(name):
            if name == "result.json" and missing:
                missing.pop()
                return None
            return original_read(name)
        with patch.object(adb, "read", side_effect=read), patch.object(control.time, "sleep"):
            self.assertTrue(control.main(["--serial", "fixture", "refresh"], lambda _: adb)["ok"])
        self.assertEqual(len(adb.sent), 1)

    def test_metadata_rejects_bools_and_unsafe_sequences(self):
        for field, value in [("pid", True), ("highwater", -1), ("highwater", 2**53), ("controlEpoch", "../file")]:
            status = FakeAdb("fixture").status
            status[field] = value
            with self.assertRaises(control.Failure):
                control.metadata(status)

if __name__ == "__main__":
    unittest.main()
