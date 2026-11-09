#!/usr/bin/env python3
"""Exercise the real Compose image and its isolated AOF volume; no user env."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[2]
KEY = chr(120) * 40  # Public, deterministic fixture material; never a user key.
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def request(path, admin=False):
    headers = {"Authorization": "Bearer " + KEY} if admin else {}
    try:
        with opener.open(urllib.request.Request("http://127.0.0.1:7400" + path, headers=headers), timeout=3) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


def wait_status(expected):
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        try:
            if request("/healthz")[0] == expected:
                return
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(0.25)
    raise RuntimeError("Compose health did not reach expected status " + str(expected))


def epoch():
    status, body = request("/api/v1/admin/overview", True)
    assert status == 200
    return json.loads(body)["coordinator"]["epoch"]


with tempfile.TemporaryDirectory(prefix="cybergrid-docker-smoke-") as temporary:
    server = Path(temporary) / "server"
    server.mkdir()
    for name in ["Dockerfile", "compose.yaml", ".dockerignore", "package.json", "package-lock.json", "tsconfig.json"]:
        shutil.copyfile(ROOT / "server" / name, server / name)
    for name in ["src", "public", "release", "notices"]:
        shutil.copytree(ROOT / "server" / name, server / name)
    (server / ".env").write_text(
        "CYBERGRID_PUBLIC_ORIGIN=https://fixture.invalid\n"
        "LOGTO_ISSUER=https://fixture.invalid/oidc\n"
        "LOGTO_BROWSER_CLIENT_ID=FixtureSPA\n"
        "LOGTO_MANAGEMENT_CLIENT_ID=FixtureDirectory\n"
        "LOGTO_MANAGEMENT_CLIENT_SECRET=" + KEY + "\n"
        "CYBERGRID_ADMIN_KEY=" + KEY + "\n"
        "CYBERGRID_REDIS_PREFIX=cybergrid:docker-smoke\n")
    command = ["docker", "compose", "--project-name", "cybergrid-ci-" + uuid.uuid4().hex[:10], "--file", str(server / "compose.yaml")]

    def compose(*args, capture=False):
        return subprocess.run(command + list(args), check=True, text=True, capture_output=capture)

    try:
        compose("up", "--build", "--detach", "--wait", "--wait-timeout", "120")
        assert compose("exec", "-T", "cybergrid", "id", "-u", capture=True).stdout.strip() != "0"
        wait_status(200)
        original = epoch()
        compose("restart", "cybergrid")
        wait_status(200)
        assert epoch() == original, "Node restart changed shared epoch"
        compose("stop", "redis")
        wait_status(503)
        compose("start", "redis")
        wait_status(200)
        assert epoch() == original, "Redis AOF restart changed shared epoch"
        print("Docker Compose PASS: non-root image, Redis-ready health, Node restart, Redis AOF epoch persistence")
    finally:
        compose("down", "--volumes", "--remove-orphans")
