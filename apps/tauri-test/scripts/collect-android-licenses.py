#!/usr/bin/env python3
"""Collect debug-runtime licenses from cached archives and public Maven POMs."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import tempfile
import urllib.request
import xml.etree.ElementTree as ET
import zipfile
from concurrent.futures import ThreadPoolExecutor

APP = Path(__file__).resolve().parents[1]
PROJECT = APP / "src-tauri/gen/android"
OUT = APP / "notices/android"
INIT = """
allprojects { p ->
  p.afterEvaluate {
    if (p.name == 'app') {
      p.tasks.register('cybergridLicenseArtifacts') {
        doLast {
          def runtime = p.configurations.findByName('universalDebugRuntimeClasspath') ?: p.configurations.getByName('debugRuntimeClasspath')
          runtime.incoming.resolutionResult.allComponents.each { a ->
            if (a.id instanceof org.gradle.api.artifacts.component.ModuleComponentIdentifier) {
              println('CYBERGRID_LICENSE ' + groovy.json.JsonOutput.toJson([group:a.id.group,name:a.id.module,version:a.id.version]))
            }
          }
        }
      }
    }
  }
}
"""


def main():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".gradle") as init:
        init.write(INIT)
        init.flush()
        output = subprocess.check_output([str(PROJECT / "gradlew"), "--offline", "--no-configuration-cache", "--console=plain", "-q", "-I", init.name, ":app:cybergridLicenseArtifacts"], cwd=PROJECT, env=os.environ).decode()
    index = []
    missing = []
    for line in output.splitlines():
        if not line.startswith("CYBERGRID_LICENSE "):
            continue
        artifact = json.loads(line.removeprefix("CYBERGRID_LICENSE "))
        cache = Path.home() / ".gradle/caches/modules-2/files-2.1" / artifact["group"] / artifact["name"] / artifact["version"]
        candidates = sorted(p for p in cache.rglob("*") if p.suffix in {".jar", ".aar"} and not p.stem.endswith(("-sources", "-javadoc")))
        if not candidates:
            # POM-only dependency constraints contain no binary to distribute.
            continue
        path = next((p for p in candidates if p.suffix == ".aar"), candidates[0])
        artifact["archiveSha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        artifact["source"] = f'https://maven.google.com/web/index.html#{artifact["group"]}:{artifact["name"]}:{artifact["version"]}' if artifact["group"].startswith(("androidx.", "com.google.android.material")) else f'https://central.sonatype.com/artifact/{artifact["group"]}/{artifact["name"]}/{artifact["version"]}'
        files = {}
        folder = OUT / artifact["group"] / f'{artifact["name"]}-{artifact["version"]}'
        with zipfile.ZipFile(path) as archive:
            for name in archive.namelist():
                if any(word in Path(name).name.lower() for word in ["license", "copying", "copyright", "notice"]) and not name.endswith("/"):
                    data = archive.read(name)
                    target = folder / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(data)
                    files[name] = hashlib.sha256(data).hexdigest()
        artifact["texts"] = files
        if not files:
            missing.append(f'{artifact["group"]}:{artifact["name"]}:{artifact["version"]}')
        index.append(artifact)
    apache_url = "https://www.apache.org/licenses/LICENSE-2.0.txt"
    common = OUT / "common/Apache-2.0.txt"
    apache = common.read_bytes() if common.is_file() else urllib.request.urlopen(apache_url, timeout=30).read()
    common.parent.mkdir(parents=True, exist_ok=True)
    common.write_bytes(apache)

    def supplement(artifact):
        if artifact["texts"]:
            return
        base = "https://dl.google.com/dl/android/maven2" if artifact["group"].startswith(("androidx.", "com.google.android.material")) else "https://repo.maven.apache.org/maven2"
        relative = f'{artifact["group"].replace(".", "/")}/{artifact["name"]}/{artifact["version"]}/{artifact["name"]}-{artifact["version"]}.pom'
        url = base + "/" + relative
        folder = OUT / artifact["group"] / f'{artifact["name"]}-{artifact["version"]}'
        cached_pom = folder / "upstream.pom"
        pom = cached_pom.read_bytes() if cached_pom.is_file() else urllib.request.urlopen(url, timeout=30).read()
        folder.mkdir(parents=True, exist_ok=True)
        (folder / "upstream.pom").write_bytes(pom)
        ns = {"m": "http://maven.apache.org/POM/4.0.0"}
        root = ET.fromstring(pom)
        licenses = [{"name": node.findtext("m:name", namespaces=ns), "url": node.findtext("m:url", namespaces=ns)} for node in root.findall("m:licenses/m:license", ns)]
        if not licenses and root.find("m:parent", ns) is not None:
            parent = root.find("m:parent", ns)
            group = parent.findtext("m:groupId", namespaces=ns)
            name = parent.findtext("m:artifactId", namespaces=ns)
            version = parent.findtext("m:version", namespaces=ns)
            parent_url = f'https://repo.maven.apache.org/maven2/{group.replace(".", "/")}/{name}/{version}/{name}-{version}.pom'
            cached_parent = folder / "parent.pom"
            parent_pom = cached_parent.read_bytes() if cached_parent.is_file() else urllib.request.urlopen(parent_url, timeout=30).read()
            cached_parent.write_bytes(parent_pom)
            licenses = [{"name": node.findtext("m:name", namespaces=ns), "url": node.findtext("m:url", namespaces=ns)} for node in ET.fromstring(parent_pom).findall("m:licenses/m:license", ns)]
            artifact["parentPomSource"] = parent_url
            artifact["parentPomSha256"] = hashlib.sha256(parent_pom).hexdigest()
        artifact["pomSource"] = url
        artifact["pomSha256"] = hashlib.sha256(pom).hexdigest()
        artifact["declaredLicenses"] = licenses
        if any("apache" in (license.get("name") or "").lower() and "2" in (license.get("name") or "") for license in licenses):
            artifact["officialLicenseText"] = {"file": "common/Apache-2.0.txt", "sha256": hashlib.sha256(apache).hexdigest(), "source": apache_url}
    with ThreadPoolExecutor(max_workers=4) as executor:
        list(executor.map(supplement, index))
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "dependency-index.json").write_text(json.dumps(index, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"runtimeArtifacts": len(index), "withoutEmbeddedLicense": len(missing), "withoutLicenseText": [f'{x["group"]}:{x["name"]}:{x["version"]}' for x in index if not x["texts"] and not x.get("officialLicenseText")]}, indent=2))


if __name__ == "__main__":
    main()
