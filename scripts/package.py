#!/usr/bin/env python3
"""Package a built binary and inspect its dynamic dependencies on the host."""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import shutil
import subprocess
import tarfile
import zipfile

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()
    args.binary.resolve(strict=True)
    folder = Path("dist") / f"celldiff-{args.target}"
    folder.mkdir(parents=True, exist_ok=True)
    binary = folder / args.binary.name
    shutil.copy2(args.binary, binary)
    for path in ["README.md", "docs/design.md", "docs/validation.md"]:
        shutil.copy2(path, folder / Path(path).name)
    system = platform.system()
    if system == "Windows":
        program = shutil.which("dumpbin")
        if not program:
            vswhere = Path("C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe")
            if vswhere.exists():
                found = subprocess.run([str(vswhere), "-latest", "-products", "*", "-find",
                    "VC/Tools/MSVC/**/bin/Hostx64/x64/dumpbin.exe"], text=True, capture_output=True)
                paths = found.stdout.strip().splitlines()
                program = paths[0] if paths else None
        command = [program, "/dependents", str(binary)] if program else None
    elif system == "Darwin":
        command = ["otool", "-L", str(binary)]
    else:
        command = ["ldd", str(binary)]
    dependency_output = "Dependency inspection tool unavailable"
    if command:
        result = subprocess.run(command, text=True, capture_output=True)
        dependency_output = result.stdout + result.stderr
    info = {
        "target": args.target, "host": platform.platform(),
        "binary_bytes": binary.stat().st_size,
        "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "dynamic_dependencies": dependency_output,
    }
    (folder / "build-info.json").write_text(json.dumps(info, indent=2) + "\n")
    if system == "Windows":
        archive = Path("dist") / f"{folder.name}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as package:
            for file in folder.iterdir():
                package.write(file, f"{folder.name}/{file.name}")
    else:
        archive = Path("dist") / f"{folder.name}.tar.gz"
        with tarfile.open(archive, "w:gz") as package:
            package.add(folder, arcname=folder.name)
    print(json.dumps(info, indent=2))

if __name__ == "__main__":
    main()
