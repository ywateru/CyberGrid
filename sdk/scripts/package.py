#!/usr/bin/env python3
"""Package only SDK-owned source, binaries and notices; never credentials/logs.

Run scripts/build.sh first. GO may select the Go installation used for metadata.
"""
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]
GO = os.environ.get("GO", "go")
VERSION = "0.4.0"
DIST = ROOT / "dist" / VERSION
THIRD = ROOT / "third_party"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def go(*args):
    return subprocess.check_output([GO, *args], cwd=ROOT).decode()


def write_json(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def modules():
    text = go("list", "-m", "-json", "all")
    decoder = json.JSONDecoder()
    while text.strip():
        value, n = decoder.raw_decode(text.lstrip())
        text = text.lstrip()[n:]
        if not value.get("Main"):
            yield value


def collect_licenses():
    if THIRD.exists():
        shutil.rmtree(THIRD)
    THIRD.mkdir()
    entries = []
    for module in modules():
        if "Dir" not in module:
            module = json.loads(go("mod", "download", "-json", module["Path"] + "@" + module["Version"]))
        module_dir = Path(module["Dir"])
        files = sorted(p for p in module_dir.iterdir() if p.is_file() and
                       p.name.upper().startswith(("LICENSE", "LICENCE", "NOTICE", "COPYING", "PATENTS")))
        if not any(p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING")) for p in files):
            raise RuntimeError("Missing license for " + module["Path"])
        target = THIRD / (module["Path"].replace("/", "_") + "@" + module["Version"])
        target.mkdir()
        for source in files:
            shutil.copyfile(source, target / source.name)
        entries.append({"module": module["Path"], "version": module["Version"], "files": [
            {"file": str((target / p.name).relative_to(ROOT)), "sha256": sha(p.read_bytes())} for p in files]})
    goroot = Path(go("env", "GOROOT").strip())
    target = THIRD / "go"
    target.mkdir()
    go_files = [goroot / name for name in ["LICENSE", "PATENTS"] if (goroot / name).is_file()]
    for source in go_files:
        shutil.copyfile(source, target / source.name)
    entries.append({"module": "Go standard library/runtime", "version": go("env", "GOVERSION").strip(), "files": [
        {"file": str((target / p.name).relative_to(ROOT)), "sha256": sha(p.read_bytes())} for p in go_files]})
    write_json(THIRD / "manifest.json", entries)
    (ROOT / "THIRD_PARTY_NOTICES.md").write_text(
        "# Third-party notices\n\n"
        "The files under `third_party/` reproduce the original license, notice and\n"
        "patent texts from every module in `go.mod`/`go.sum`'s resolved module graph,\n"
        "including transitive and test dependencies, plus the Go runtime.\n"
        "`third_party/manifest.json` records exact module versions and text hashes.\n\n"
        "A transitive Pion dependency includes TURN implementation code; CyberGrid\n"
        "does not configure or use TURN relaying. The Android CLI dynamically uses\n"
        "the device's bionic system libraries; it does not bundle Android system\n"
        "libraries or the NDK. cloudflared is a separate client-side program bundled\n"
        "as fixed platform resources under provider-assets/, with its original\n"
        "license, linked-module notices, source/patch pins and offline packaging.\n\n"
        "Original CyberGrid code has no separately granted open-source license;\n"
        "see LICENSE. Third-party license grants remain in effect.\n")


def archive(name, files, prefix, extra=None):
    # Stable gzip/tar metadata prevents local user names and paths leaking.
    path = DIST / name
    with path.open("wb") as output:
        with gzip.GzipFile(fileobj=output, mode="wb", filename="", mtime=0) as zipped:
            with tarfile.open(fileobj=zipped, mode="w") as tar:
                for file, relative in sorted(files, key=lambda item: item[1]):
                    b = file.read_bytes()
                    info = tarfile.TarInfo(prefix + "/" + relative)
                    info.size = len(b)
                    info.mode = 0o755 if file.name.startswith("cybergrid-") or file.name.endswith((".sh", ".py")) else 0o644
                    tar.addfile(info, io.BytesIO(b))
                if extra:
                    b = (json.dumps(extra, indent=2) + "\n").encode()
                    info = tarfile.TarInfo(prefix + "/BUILD-INFO.json")
                    info.size = len(b)
                    info.mode = 0o644
                    tar.addfile(info, io.BytesIO(b))
    return {"file": name, "bytes": path.stat().st_size, "sha256": sha(path.read_bytes())}


def main():
    DIST.mkdir(parents=True,exist_ok=True)
    collect_licenses()
    source_files = sorted([p for p in ROOT.rglob("*.go") if not {"bin","dist","archive"}.intersection(p.relative_to(ROOT).parts)] + [ROOT / "go.mod", ROOT / "go.sum"])
    provider_files = sorted(p for p in (ROOT / "provider-assets").rglob("*") if p.is_file())
    source_hash = hashlib.sha256()
    for path in sorted(source_files + provider_files):
        source_hash.update(str(path.relative_to(ROOT)).encode() + b"\0" + path.read_bytes() + b"\0")
    artifacts = []
    for name in ["cybergrid-macos", "cybergrid-android-arm64"]:
        path = ROOT / "bin" / name
        artifacts.append({"file": name, "bytes": path.stat().st_size, "sha256": sha(path.read_bytes())})
    manifest = {
        "version": VERSION, "sourceDateEpoch": int(os.environ.get("SOURCE_DATE_EPOCH", "0")),
        "goVersion": go("env", "GOVERSION").strip(), "module": "github.com/ywateru/CyberGrid/sdk",
        "sourceTreeSHA256": source_hash.hexdigest(),
        "sourceHashMethod": "sorted relative path + NUL + file bytes + NUL; all .go plus go.mod/go.sum and fixed provider-assets/**",
        "artifacts": artifacts,
        "builds": {
            "cybergrid-macos": {"GOOS": "darwin", "GOARCH": "arm64", "command": "go build -trimpath -o bin/cybergrid-macos ./cmd/cybergrid"},
            "cybergrid-android-arm64": {"GOOS": "android", "GOARCH": "arm64", "CGO_ENABLED": "1", "ndk": "27.2.12479018", "api": 28,
                "command": "CGO_ENABLED=1 GOOS=android GOARCH=arm64 CC=<NDK>/aarch64-linux-android28-clang go build -trimpath -ldflags=-checklinkname=0 -o bin/cybergrid-android-arm64 ./cmd/cybergrid"}},
        "license": "Original CyberGrid code: all rights reserved; third-party grants and notices preserved",
        "thirdPartyManifestSHA256": sha((THIRD / "manifest.json").read_bytes()),
        "bundledProviderManifestSHA256": sha((ROOT / "provider-assets/manifest.json").read_bytes()),
    }
    notices = [(ROOT / name, name) for name in ["README.md", "CHANGELOG.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]]
    notices += [(p, str(p.relative_to(ROOT))) for p in THIRD.rglob("*") if p.is_file()]
    packages = []
    source = source_files + provider_files + sorted(p for p in (ROOT / "scripts").rglob("*") if p.is_file())
    packages.append(archive(f"cybergrid-sdk-{VERSION}-source.tar.gz", [(p, str(p.relative_to(ROOT))) for p in source] + notices, f"cybergrid-sdk-{VERSION}", manifest))
    for name, platform in [("cybergrid-macos", "macos-arm64"), ("cybergrid-android-arm64", "android-arm64")]:
        packages.append(archive(f"cybergrid-sdk-{VERSION}-{platform}.tar.gz", [(ROOT / "bin" / name, name)] + [(p,str(p.relative_to(ROOT))) for p in provider_files if "darwin-arm64/cloudflared" not in str(p) and (platform == "android-arm64" or "android-arm64/jniLibs" not in str(p))] + [(ROOT / "scripts/prepare-android-provider.py", "scripts/prepare-android-provider.py")] + notices, f"cybergrid-sdk-{VERSION}-{platform}", manifest))
    manifest["packages"] = packages
    write_json(ROOT / "bin" / "manifest.json", manifest)
    write_json(DIST / "manifest.json", manifest)
    (ROOT / "bin" / "SHA256SUMS").write_text("".join(f"{a['sha256']}  {a['file']}\n" for a in artifacts))
    (DIST / "SHA256SUMS").write_text("".join(f"{p['sha256']}  {p['file']}\n" for p in packages) + f"{sha((DIST / 'manifest.json').read_bytes())}  manifest.json\n")
    for package in packages:
        print(f"{package['file']} {package['bytes']} bytes {package['sha256']}")


if __name__ == "__main__":
    main()
