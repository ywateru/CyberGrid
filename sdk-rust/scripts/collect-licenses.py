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
supplemental_repos = {
    "asn1-rs-impl-0.2.0": "rusticata/asn1-rs",
    "yasna-0.5.2": "qnighy/yasna.rs",
    "webrtc-0.14.0": "webrtc-rs/webrtc",
}
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
        repo = supplemental_repos.get(label)
        if not repo or not vcs or not vcs.get("git", {}).get("sha1"):
            raise RuntimeError("Missing exact-source license texts: " + label)
        revision = vcs["git"]["sha1"]
        for filename in ["LICENSE-MIT", "LICENSE-APACHE"]:
            url = f"https://raw.githubusercontent.com/{repo}/{revision}/{filename}"
            target = dest / "upstream-repository-root" / filename
            if target.exists():
                data = target.read_bytes()
            else:
                data = urllib.request.urlopen(url, timeout=30).read()
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            texts.append({"path": str(target.relative_to(out)), "origin": "pinned-upstream-repository", "url": url, "sha256": hashlib.sha256(data).hexdigest()})
    record = {"name": package["name"], "version": package["version"], "declaredLicense": package.get("license"), "authors": package.get("authors", []), "repository": package.get("repository"), "cargoSource": package["source"], "crateChecksum": checksums[(package["name"], package["version"])], "vcs": vcs, "targets": selected[ident], "texts": texts}
    (dest / "SOURCE.json").write_text(json.dumps(record, sort_keys=True, indent=2) + "\n")
    records.append(record)
manifest = {"schemaVersion": 1, "targets": targets, "scope": "Cargo locked reachable target graph, conservatively including dev/build dependencies", "packages": records}
(out / "MANIFEST.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
index = ["# Dependency license and notice texts", "", "Verbatim files from the Cargo registry cache for the locked SDK dependency graph reachable on macOS arm64 or Android arm64. Includes build/dev dependencies as a conservative superset. Each package has SOURCE.json with crate checksum, Cargo source, available upstream commit and per-text SHA256. MANIFEST.json is the complete machine-readable index.", "", "asn1-rs-impl 0.2.0, yasna 0.5.2 and webrtc 0.14.0 omit license texts from their crate archives. Their MIT and Apache texts were retrieved from repository roots at the exact upstream commits recorded in those archives; SOURCE.json records immutable URLs. All other texts came from existing cache. Original bytes and nested paths are preserved, including COPYRIGHT, NOTICE, AUTHORS and nested LICENSES directories.", "", "The original THIRD_PARTY_NOTICES.md retains the wider all-target Cargo.lock metadata list; packages exclusive to other architectures/OSes are outside this supported-target text collection. cloudflared is a separate provider and its notices belong to its own distribution.", "", "| Package | Declared license | Text files |", "| --- | --- | --- |"]
for record in records:
    label = record["name"] + "-" + record["version"]
    paths = ", ".join(f"[{Path(text['path']).name}]({text['path']})" for text in record["texts"])
    license_value = (record["declaredLicense"] or "See texts").replace("|", "\\|")
    index.append(f"| {label} | {license_value} | {paths} |")
(out / "README.md").write_text("\n".join(index) + "\n")
print(f"Collected {len(records)} packages and {sum(len(p['texts']) for p in records)} verbatim text files")
