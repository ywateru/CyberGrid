#!/usr/bin/env python3
"""Collect verbatim cached dependency notices for the SDK's supported targets.

Uses the locked graph without building. Missing crate-packaged texts are fetched
only from the exact upstream commit recorded in the crate's .cargo_vcs_info.json.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import urllib.request
import urllib.parse

root = Path(__file__).resolve().parents[1]
out = root / "licenses"
out.mkdir(exist_ok=True)
targets = ["aarch64-apple-darwin", "aarch64-linux-android"]
selected = {}
packages = {}
for target in targets:
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--offline", "--locked", "--format-version", "1",
        "--filter-platform", target, "--manifest-path", str(root / "Cargo.toml")]))
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    todo = [metadata["resolve"]["root"]]
    seen = set()
    while todo:
        ident = todo.pop()
        if ident in seen:
            continue
        seen.add(ident)
        todo.extend(dep["pkg"] for dep in nodes[ident]["deps"])
    for ident in seen:
        selected.setdefault(ident, []).append(target)
    packages.update({p["id"]: p for p in metadata["packages"]})
checksums = {}
for section in (root / "Cargo.lock").read_text().split("[[package]]")[1:]:
    fields = dict(re.findall(r'^([a-z_]+) = "([^"\n]*)"$', section, re.M))
    checksums[(fields["name"], fields["version"])] = fields.get("checksum")
upstream_cache = {}
def upstream_texts(package, vcs, dest):
    repository = (package.get("repository") or "").removesuffix(".git").rstrip("/")
    match = re.match(r"^https://github\.com/([^/]+/[^/]+)(?:/.*)?$", repository)
    revision = (vcs or {}).get("git", {}).get("sha1", "")
    if not match or not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise RuntimeError("Missing exact upstream commit: " + package["name"])
    if package["name"] == "enum-assoc" and package["version"] == "1.4.2":
        if revision != "5d732f47b429d382daffb0948770adbaa2de0b8c" or package.get("license") != "MIT OR Apache-2.0":
            raise RuntimeError("Review changed enum-assoc license provenance")
        declaration_url = f"https://raw.githubusercontent.com/Eolu/enum-assoc/{revision}/Cargo.toml"
        declaration = urllib.request.urlopen(declaration_url, timeout=30).read()
        original = (Path(package["manifest_path"]).parent / "Cargo.toml.orig").read_bytes()
        if declaration != original:
            raise RuntimeError("Exact upstream Cargo declaration differs from crate original")
        standard_url = "https://www.apache.org/licenses/LICENSE-2.0.txt"
        standard = dest / "standard-license-text" / "Apache-2.0.txt"
        data = standard.read_bytes() if standard.exists() else urllib.request.urlopen(standard_url, timeout=30).read()
        texts = []
        for relative, content, origin, url in [("standard-license-text/Apache-2.0.txt", data, "standardized-license-text-not-upstream-original", standard_url), ("declared-license-source/Cargo.toml.orig", declaration, "pinned-upstream-license-declaration", declaration_url)]:
            target = dest / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
            texts.append({"path": str(target.relative_to(out)), "origin": origin, "url": url, "sha256": hashlib.sha256(content).hexdigest()})
        return texts
    repo = match[1]
    cached = dest / "SOURCE.json"
    if cached.exists():
        previous = json.loads(cached.read_text())
        if previous.get("vcs") == vcs and previous.get("texts") and all(t.get("origin") == "pinned-upstream-repository" and (out / t["path"]).is_file() and hashlib.sha256((out / t["path"]).read_bytes()).hexdigest() == t["sha256"] for t in previous["texts"]):
            return previous["texts"]
    key = (repo, revision)
    if key not in upstream_cache:
        url = f"https://api.github.com/repos/{repo}/contents?ref={revision}"
        entries = json.load(urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "CyberGrid-license-collector"}), timeout=30))
        for entry in list(entries):
            if entry["type"] == "dir" and entry["name"].lower() in ["licenses", "licences"]:
                child = f'https://api.github.com/repos/{repo}/contents/{entry["path"]}?ref={revision}'
                entries.extend(json.load(urllib.request.urlopen(urllib.request.Request(child, headers={"User-Agent": "CyberGrid-license-collector"}), timeout=30)))
        files = {}
        for entry in entries:
            if entry["type"] != "file" or not (pattern.match(entry["name"]) or any(part.lower() in ["licenses", "licences"] for part in Path(entry["path"]).parts[:-1])):
                continue
            url = f'https://raw.githubusercontent.com/{repo}/{revision}/{urllib.parse.quote(entry["path"])}'
            files[entry["path"]] = (urllib.request.urlopen(url, timeout=30).read(), url)
        if not files:
            raise RuntimeError("No license texts at exact upstream commit: " + package["name"])
        upstream_cache[key] = files
    texts = []
    for filename, (data, url) in upstream_cache[key].items():
        target = dest / "upstream-repository-root" / filename
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        texts.append({"path": str(target.relative_to(out)), "origin": "pinned-upstream-repository", "url": url, "sha256": hashlib.sha256(data).hexdigest()})
    return texts
pattern = re.compile(r"^(licen[cs]e|copying|copyright|notice|unlicen[cs]e|authors)(?:$|[._-])", re.I)
records = []
for ident in sorted(selected, key=lambda key: (packages[key]["name"], packages[key]["version"])):
    package = packages[ident]
    if package["name"] == "cybergrid-sdk":
        continue
    label = package["name"] + "-" + package["version"]
    source = Path(package["manifest_path"]).parent
    dest = out / label
    dest.mkdir(exist_ok=True)
    vcs_file = source / ".cargo_vcs_info.json"
    vcs = json.loads(vcs_file.read_text()) if vcs_file.exists() else None
    texts = []
    for path in sorted(source.rglob("*")):
        rel = path.relative_to(source)
        licensedir = any(part.lower() in ["licenses", "licences"] for part in rel.parts[:-1])
        if path.is_symlink() or not path.is_file() or not (pattern.match(path.name) or licensedir or str(rel) == package.get("license_file")):
            continue
        data = path.read_bytes()
        target = dest / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        texts.append({"path": str(target.relative_to(out)), "origin": "cargo-registry-cache", "cratePath": str(rel), "sha256": hashlib.sha256(data).hexdigest()})
    if not texts:
        texts = upstream_texts(package, vcs, dest)
    elif package["name"] in ["iroh", "iroh-relay"] and package["version"] == "1.4.1":
        # Their packaged BSD3 text covers Tailscale-derived socket code only.
        # Preserve it and include the workspace's separately declared primary licenses.
        if (vcs or {}).get("git", {}).get("sha1") != "d6c6963b9602d1b8dd2ba53b6519e7eb19cc7940":
            raise RuntimeError("Review changed iroh workspace license provenance")
        texts.extend(upstream_texts(package, vcs, dest))
    record = {"name": package["name"], "version": package["version"], "declaredLicense": package.get("license"), "authors": package.get("authors", []), "repository": package.get("repository"), "cargoSource": package["source"], "crateChecksum": checksums[(package["name"], package["version"])], "vcs": vcs, "targets": selected[ident], "texts": texts}
    if package["name"] == "enum-assoc" and package["version"] == "1.4.2":
        record["licenseSelection"] = {"selectedLicense": "Apache-2.0", "upstreamOriginalLicenseTextAbsent": True, "explanation": "The exact crate and upstream commit declare MIT OR Apache-2.0 but include no original license text. This distribution selects the offered Apache-2.0 alternative and includes the canonical versioned standard text, explicitly not an upstream original. Published authors are retained verbatim from Cargo metadata; no copyright year or notice is invented."}
    (dest / "SOURCE.json").write_text(json.dumps(record, sort_keys=True, indent=2) + "\n")
    records.append(record)
manifest = {"schemaVersion": 1, "targets": targets, "scope": "Cargo locked reachable target graph, conservatively including dev/build dependencies", "packages": records}
(out / "MANIFEST.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
index = ["# Dependency license and notice texts", "", "Verbatim files from the Cargo registry cache for the locked SDK dependency graph reachable on macOS arm64 or Android arm64. Includes build/dev dependencies as a conservative superset. Each package has SOURCE.json with crate checksum, Cargo source, available upstream commit and per-text SHA256. MANIFEST.json is the complete machine-readable index.", "", "Some packages omit license texts from their crate archives. For those packages, texts were retrieved only from repository roots at the exact upstream commits recorded in those archives; SOURCE.json records immutable URLs. Other texts came from the Cargo cache. enum-assoc 1.4.2 provides no original license text at its exact published commit; this distribution selects its declared Apache-2.0 alternative and includes explicitly labelled canonical standard text and its verbatim exact-commit Cargo declaration. SOURCE.json records that exception without inventing copyright notices. Original bytes and nested paths are preserved, including COPYRIGHT, NOTICE, AUTHORS and nested LICENSES directories.", "", "The original THIRD_PARTY_NOTICES.md retains the wider all-target Cargo.lock metadata list; packages exclusive to other architectures/OSes are outside this supported-target text collection. cloudflared is a separate provider and its notices belong to its own distribution.", "", "| Package | Declared license | Text files |", "| --- | --- | --- |"]
for record in records:
    label = record["name"] + "-" + record["version"]
    paths = ", ".join(f"[{Path(text['path']).name}]({text['path']})" for text in record["texts"])
    license_value = (record["declaredLicense"] or "See texts").replace("|", "\\|")
    index.append(f"| {label} | {license_value} | {paths} |")
(out / "README.md").write_text("\n".join(index) + "\n")
print(f"Collected {len(records)} packages and {sum(len(p['texts']) for p in records)} verbatim text files")
