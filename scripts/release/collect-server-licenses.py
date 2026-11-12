#!/usr/bin/env python3
"""Collect lockfile-pinned npm license texts for every Node server target."""
import base64
import hashlib
import io
import json
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
SERVER = ROOT / "server"
PREFIXES = ("license", "licence", "notice", "copyright", "copying", "unlicense")


def public_bytes(url):
    # No credentials or headers are supplied; curl also handles the host's
    # ordinary TLS/proxy setup consistently with npm and the release tooling.
    return subprocess.check_output(['curl', '--fail', '--silent', '--show-error',
                                    '--location', '--max-time', '30', '--retry', '2',
                                    '--max-filesize', str(10 * 1024 * 1024), url])


def local_texts(folder):
    return {p.name: p.read_bytes() for p in folder.iterdir()
            if p.is_file() and p.name.lower().startswith(PREFIXES)}


def archive_texts(meta, name):
    metadata = json.loads(public_bytes(f'https://registry.npmjs.org/{name}/{meta["version"]}'))
    if metadata["dist"]["integrity"] != meta["integrity"]:
        raise RuntimeError("Official npm metadata differs from lockfile integrity")
    url = metadata["dist"]["tarball"]
    if not url.startswith("https://registry.npmjs.org/"):
        raise RuntimeError("Only exact public npm registry archives are supported")
    data = public_bytes(url)
    if len(data) > 10 * 1024 * 1024:
        raise RuntimeError("License archive exceeds the bounded download size")
    algorithm, expected = meta["integrity"].split("-", 1)
    if algorithm != "sha512" or base64.b64encode(hashlib.sha512(data).digest()).decode() != expected:
        raise RuntimeError("npm archive integrity mismatch")
    result = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            path = Path(member.name)
            if member.isfile() and len(path.parts) == 2 and path.name.lower().startswith(PREFIXES):
                result[path.name] = archive.extractfile(member).read()
    # Provenance describes the published package, independent of whether this
    # OS installed it or its pinned archive supplied the same original text.
    source = {"package": name, "version": meta["version"]}
    if not result and (name == "redis" or name.startswith("@redis/")) and meta["version"] == "6.3.1":
        commit = metadata.get("gitHead", "")
        if not re.fullmatch('[0-9a-f]{40}', commit) or metadata.get('repository', {}).get('url') != 'git://github.com/redis/node-redis.git':
            raise RuntimeError("Review changed Redis package license provenance")
        url = f'https://raw.githubusercontent.com/redis/node-redis/{commit}/LICENSE'
        result["upstream-repository/LICENSE"] = public_bytes(url)
        source.update({"archive": metadata["dist"]["tarball"], "gitCommit": commit, "licenseURL": url,
                       "reason": "Published npm archive omits license body; original MIT text from its exact npm gitHead."})
    return result, source


def collect(out):
    lock = json.loads((SERVER / "package-lock.json").read_text())
    packages = []
    for path, meta in sorted(lock["packages"].items()):
        if not path:
            continue
        name = path.split("node_modules/")[-1]
        version = meta["version"]
        folder = SERVER / path
        source = {"package": name, "version": version}
        if folder.is_dir():
            actual = json.loads((folder / "package.json").read_text())
            if actual["version"] != version:
                raise RuntimeError("Installed package differs from lockfile: " + name)
            texts = local_texts(folder)
        else:
            texts = {}
        if not texts and name.startswith("@esbuild/"):
            wrapper = SERVER / "node_modules/esbuild"
            if json.loads((wrapper / "package.json").read_text())["version"] != version:
                raise RuntimeError("esbuild optional package and wrapper versions differ")
            texts = {"upstream-wrapper/" + key: value for key, value in local_texts(wrapper).items()}
            source = {"package": "esbuild", "version": version,
                      "reason": "Same-version wrapper distributes the upstream platform binary license."}
        if not texts:
            texts, source = archive_texts(meta, name)
        if not texts:
            raise RuntimeError("Missing original license text: " + name)
        destination = out / "npm" / (name.replace("/", "__") + "-" + version)
        hashes = {}
        for relative, data in sorted(texts.items()):
            file = destination / relative
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(data)
            hashes[relative] = hashlib.sha256(data).hexdigest()
        packages.append({"name": name, "version": version, "license": meta.get("license"),
                         "archive": meta.get("resolved"), "integrity": meta.get("integrity"),
                         "developmentOnly": bool(meta.get("dev")), "optional": bool(meta.get("optional")),
                         "licenseSource": source, "texts": hashes})
    (out / "manifest.json").write_text(json.dumps({"packages": packages}, indent=2) + "\n")
    (out / "README.md").write_text(
        "# Node server third-party notices\n\n"
        "Original license texts for every package in server/package-lock.json, including optional "
        "platform packages and build dependencies. Texts retain upstream bytes. Redis client "
        "packages use the MIT license; Redis service terms are separate.\n\n"
        "Regenerate with `python3 scripts/release/collect-server-licenses.py` after `npm ci --prefix server`. "
        "Missing installed packages are read only from integrity-verified, lockfile-pinned public npm archives.\n")
    return len(packages)


def tree(folder):
    return {p.relative_to(folder).as_posix(): p.read_bytes() for p in folder.rglob("*") if p.is_file()}


def main():
    with tempfile.TemporaryDirectory(prefix="cybergrid-server-notices-") as temporary:
        generated = Path(temporary)
        count = collect(generated)
        destination = SERVER / "notices"
        if "--check" in sys.argv:
            if tree(destination) != tree(generated):
                raise SystemExit("Server notices differ from the current lockfile and original texts")
        else:
            destination.mkdir(exist_ok=True)
            shutil.copytree(generated, destination, dirs_exist_ok=True)
        print(f"Server notices verified: {count} lockfile packages")


if __name__ == "__main__":
    main()
