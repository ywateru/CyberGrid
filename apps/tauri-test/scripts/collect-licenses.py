#!/usr/bin/env python3
"""Collect actual cached dependency license texts; no credential or network access."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import urllib.parse
import urllib.request

APP = Path(__file__).resolve().parents[1]
OUT = APP / "notices"
PREFIXES = ("license", "licence", "notice", "copyright", "copying", "unlicense", "authors")
UPSTREAM_CACHE = {}


def texts(root, explicit=None):
    found = set()
    if explicit and (root / explicit).is_file():
        found.add(root / explicit)
    for path in root.rglob("*"):
        if path.is_file() and path.name.lower().startswith(PREFIXES) and path.suffix not in {".rs", ".c", ".h", ".toml", ".json"}:
            found.add(path)
    return sorted(found)


def record(root, paths, folder):
    result = {}
    for path in paths:
        rel = path.relative_to(root)
        dest = folder / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, dest)
        result[str(rel)] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def upstream_texts(package, vcs, folder):
    """Fetch only public license text from the crate's exact published commit."""
    repository = package.get("repository", "").removesuffix(".git").rstrip("/")
    commit = vcs.get("git", {}).get("sha1") if vcs else None
    if not repository.startswith("https://github.com/") or not commit:
        return {}
    if package["name"] == "enum-assoc" and package["version"] == "1.4.2":
        sdk_root = APP.parents[1] / "sdk-rust" / "licenses"
        source = json.loads((sdk_root / "enum-assoc-1.4.2/SOURCE.json").read_text())
        if source.get("vcs") != vcs or source.get("licenseSelection", {}).get("selectedLicense") != "Apache-2.0" or not source["licenseSelection"].get("upstreamOriginalLicenseTextAbsent"):
            raise RuntimeError("Run SDK license collection and review enum-assoc provenance first")
        hashes, sources = {}, {}
        for text in source["texts"]:
            path = sdk_root / text["path"]
            data = path.read_bytes()
            if hashlib.sha256(data).hexdigest() != text["sha256"]:
                raise RuntimeError("SDK license text checksum mismatch")
            relative = Path(text["path"]).relative_to("enum-assoc-1.4.2")
            destination = folder / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
            hashes[str(relative)] = text["sha256"]
            sources[str(relative)] = text["url"]
        (folder / "UPSTREAM.json").write_text(json.dumps({"gitCommit": commit, "sources": sources, "licenseSelection": source["licenseSelection"], "authors": source["authors"]}, indent=2) + "\n")
        return hashes
    existing = folder / "UPSTREAM.json"
    if existing.is_file():
        cached = json.loads(existing.read_text())
        if cached.get("gitCommit") == commit and cached.get("sources"):
            return {name: hashlib.sha256((folder / name).read_bytes()).hexdigest() for name in cached["sources"]}
    repo = repository.removeprefix("https://github.com/")
    key = (repo, commit)
    if key not in UPSTREAM_CACHE:
        url = f"https://api.github.com/repos/{repo}/contents?ref={commit}"
        request = urllib.request.Request(url, headers={"User-Agent": "CyberGrid-license-collector"})
        entries = json.load(urllib.request.urlopen(request, timeout=30))
        result = {}
        for entry in list(entries):
            if entry["type"] == "dir" and entry["name"].lower().startswith(PREFIXES):
                request = urllib.request.Request(f'https://api.github.com/repos/{repo}/contents/{entry["path"]}?ref={commit}', headers={"User-Agent": "CyberGrid-license-collector"})
                entries.extend(json.load(urllib.request.urlopen(request, timeout=30)))
        for entry in entries:
            if entry["type"] == "file" and entry["name"].lower().startswith(PREFIXES):
                raw_url = f'https://raw.githubusercontent.com/{repo}/{commit}/{urllib.parse.quote(entry["path"])}'
                result[entry["path"]] = (urllib.request.urlopen(raw_url, timeout=30).read(), raw_url)
            elif entry["type"] == "file" and any(p.lower().startswith(PREFIXES) for p in Path(entry["path"]).parts[:-1]):
                raw_url = f'https://raw.githubusercontent.com/{repo}/{commit}/{urllib.parse.quote(entry["path"])}'
                result[entry["path"]] = (urllib.request.urlopen(raw_url, timeout=30).read(), raw_url)
        if not result and package.get("license") == "MPL-2.0":
            # Some source mirrors carry only per-file MPL headers. Distribute
            # Mozilla's actual versioned text, with its official source URL.
            raw_url = "https://www.mozilla.org/media/MPL/2.0/index.815ca599c9df.txt"
            result["MPL-2.0.txt"] = (urllib.request.urlopen(raw_url, timeout=30).read(), raw_url)
        UPSTREAM_CACHE[key] = result
    result = {}
    sources = {}
    for name, (data, url) in UPSTREAM_CACHE[key].items():
        relative = Path("upstream-repository-root") / name
        destination = folder / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        result[str(relative)] = hashlib.sha256(data).hexdigest()
        sources[str(relative)] = url
    if sources:
        (folder / "UPSTREAM.json").write_text(json.dumps({"gitCommit": commit, "sources": sources}, indent=2) + "\n")
    return result


def main():
    reachable = set()
    packages = {}
    for target in ["aarch64-apple-darwin", "aarch64-linux-android"]:
        metadata = json.loads(subprocess.check_output([
            "cargo", "metadata", "--locked", "--offline", "--format-version", "1",
            "--filter-platform", target, "--manifest-path", str(APP / "src-tauri/Cargo.toml"),
        ]))
        packages.update({p["id"]: p for p in metadata["packages"]})
        nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
        todo = [metadata["resolve"]["root"]]
        visited = set()
        while todo:
            node_id = todo.pop()
            if node_id in visited:
                continue
            visited.add(node_id)
            todo.extend(dep["pkg"] for dep in nodes[node_id]["deps"])
        reachable.update(visited)
    OUT.mkdir(exist_ok=True)
    index = []
    missing = []
    for package in sorted(packages.values(), key=lambda p: (p["name"], p["version"])):
        if package["source"] is None or package["id"] not in reachable:
            continue
        root = Path(package["manifest_path"]).parent
        folder = OUT / "rust" / f'{package["name"]}-{package["version"]}'
        files = record(root, texts(root, package.get("license_file")), folder)
        vcs = root / ".cargo_vcs_info.json"
        vcs_info = json.loads(vcs.read_text()) if vcs.is_file() else None
        primary_iroh_missing = package["name"] in ["iroh", "iroh-relay"] and package["version"] == "1.4.1"
        if primary_iroh_missing and (vcs_info or {}).get("git", {}).get("sha1") != "d6c6963b9602d1b8dd2ba53b6519e7eb19cc7940":
            raise RuntimeError("Review changed iroh workspace license provenance")
        if (not files or primary_iroh_missing) and "--fetch-upstream" in sys.argv:
            # The packaged BSD3 text is supplemental socket-code licensing,
            # not the workspace's primary MIT/Apache declarations.
            files.update(upstream_texts(package, vcs_info, folder))
        checksum_file = root / ".cargo-checksum.json"
        checksum = json.loads(checksum_file.read_text()).get("package") if checksum_file.is_file() else None
        item = {"ecosystem": "cargo", "name": package["name"], "version": package["version"], "license": package.get("license"), "source": package["source"], "crateArchiveChecksumSha256": checksum, "repository": package.get("repository"), "vcs": vcs_info, "texts": files}
        if package["name"] == "enum-assoc" and package["version"] == "1.4.2" and (folder / "UPSTREAM.json").exists():
            exception = json.loads((folder / "UPSTREAM.json").read_text())
            item["licenseSelection"] = exception["licenseSelection"]
            item["authors"] = exception["authors"]
        index.append(item)
        if not files:
            missing.append(f'{package["name"]}-{package["version"]}')
    for package_json in sorted((APP / "node_modules").rglob("package.json")):
        if "node_modules" in package_json.relative_to(APP / "node_modules").parts[:-2]:
            continue
        root = package_json.parent
        package = json.loads(package_json.read_text())
        if not package.get("name") or not package.get("version"):
            continue
        name, version = package["name"], package["version"]
        folder = OUT / "npm" / f'{name.replace("/", "__")}-{version}'
        files = record(root, texts(root), folder)
        item = {"ecosystem": "npm", "name": name, "version": version, "license": package.get("license"), "repository": package.get("repository"), "texts": files}
        if not files:
            wrapper = "esbuild" if name.startswith("@esbuild/") else "rollup" if name.startswith("@rollup/") else "@tauri-apps/cli" if name.startswith("@tauri-apps/cli-") else None
            if wrapper:
                wrapper_root = APP / "node_modules" / wrapper
                wrapper_package = json.loads((wrapper_root / "package.json").read_text())
                assert wrapper_package["version"] == version, "Native tool/wrapper versions differ"
                copied = record(wrapper_root, texts(wrapper_root), folder / "upstream-wrapper")
                files.update({str(Path("upstream-wrapper") / p): sha for p, sha in copied.items()})
                item["licenseSourcePackage"] = {"name": wrapper, "version": version, "repository": wrapper_package.get("repository"), "reason": "Same-version upstream tool wrapper distributes the license for its platform-specific optional binary package."}
        index.append(item)
    provider = APP.parents[1] / "sdk-rust/provider-assets"
    provider_out = OUT / "cloudflared"
    provider_files = [provider / name for name in [
        "README.md", "LICENSE", "THIRD_PARTY_NOTICES.md", "manifest.json",
        "android-build-manifest.json", "build-modules-android-arm64.txt",
        "build-modules-darwin-arm64.txt", "patches/0001-android-explicit-dns.patch",
        "rebuild/build-android.sh", "rebuild/write-manifest.py",
        "rebuild/patches/0001-android-explicit-dns.patch",
    ]]
    assert all(path.is_file() and not path.is_symlink() for path in provider_files), "SDK provider notices are incomplete"
    if provider_out.exists():
        shutil.rmtree(provider_out)
    provider_hashes = record(provider, provider_files, provider_out)
    (provider_out / "source-files-sha256.json").write_text(json.dumps(provider_hashes, indent=2, sort_keys=True) + "\n")
    (OUT / "dependency-index.json").write_text(json.dumps(index, indent=2, sort_keys=True) + "\n")
    (OUT / "README.md").write_text(
        "# Dependency notices\n\n"
        "Actual LICENSE / COPYING / COPYRIGHT / NOTICE texts copied unchanged from the locked Cargo registry and installed npm packages. "
        "dependency-index.json records package version, upstream source and SHA256 of each copied file. "
        "The collection includes build/target-specific dependencies, so it may be broader than either binary. "
        "enum-assoc 1.4.2 has no original license text at its exact published commit: this distribution selects its declared Apache-2.0 alternative and includes explicitly labelled canonical standard text plus its original Cargo declaration and published author attribution, without inventing a copyright notice. "
        "Cloudflared's Apache-2.0 license, third-party notices, dual-platform resource manifest and exact upstream source/build records are copied unchanged from sdk-rust/provider-assets into notices/cloudflared. "
        "That folder's source-files-sha256.json binds the copied texts. The source archive also includes the SDK's pinned macOS/Android provider bytes and offline packaging/rebuild scripts.\n"
    )
    print(json.dumps({"packages": len(index), "cargoPackagesWithoutRootLicenseText": missing}, indent=2))


if __name__ == "__main__":
    main()
