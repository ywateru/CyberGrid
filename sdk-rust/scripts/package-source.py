#!/usr/bin/env python3
"""Deterministic, allowlisted source delivery. No cloud credentials or build outputs; fixed provider resources are included."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile

root = Path(__file__).resolve().parents[1]
version = "0.2.0"
prefix = f"cybergrid-sdk-rust-{version}"
files = [root / name for name in ["Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "THIRD_PARTY_NOTICES.md"]]
for directory, pattern in [("src", "*.rs"), ("examples", "*.rs"), ("docs", "*.md"), ("scripts", "*.py")]:
    files.extend(sorted((root / directory).glob(pattern)))
files.extend(sorted(path for path in (root / "licenses").rglob("*") if path.is_file()))
files.extend(sorted(path for path in (root / "provider-assets").rglob("*") if path.is_file()))
files = sorted(set(files))
for path in files:
    if path.is_symlink() or not path.is_file() or not path.is_relative_to(root):
        raise RuntimeError(f"Refusing non-regular source: {path}")
entries = {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}
manifest = (json.dumps({"name": "cybergrid-sdk", "version": version, "protocol": 1, "sha256": entries}, sort_keys=True, indent=2) + "\n").encode()
raw = io.BytesIO()
with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as tar:
    for name, data in [(str(p.relative_to(root)), p.read_bytes()) for p in files] + [("SOURCE-MANIFEST.json", manifest)]:
        info = tarfile.TarInfo(f"{prefix}/{name}")
        info.size = len(data)
        info.mtime = 0
        info.mode = 0o755 if name.endswith((".sh", ".py")) else 0o644
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        tar.addfile(info, io.BytesIO(data))
out = root / "dist" / version
out.mkdir(parents=True, exist_ok=True)
archive = out / f"{prefix}.tar.gz"
with archive.open("wb") as stream:
    with gzip.GzipFile(filename="", mode="wb", fileobj=stream, mtime=0, compresslevel=9) as compressed:
        compressed.write(raw.getvalue())
(out / "SOURCE-MANIFEST.json").write_bytes(manifest)
checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
(out / "SHA256SUMS").write_text(f"{checksum}  {archive.name}\n")
print(f"{archive}\nSHA256 {checksum}\n{len(files)} source files; fixed provider resources included; no dependency builds, credentials or build outputs")
