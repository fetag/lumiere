#!/usr/bin/env python3
"""將 IINA Frameworks 中 libmpv 所需的 dylib 複製到 Lumiere.app。"""
import shutil
import subprocess
import sys
from pathlib import Path

source = Path(sys.argv[1]).resolve()
destination = Path(sys.argv[2])
destination.mkdir(parents=True, exist_ok=True)
pending = ["libmpv.2.dylib"]
copied = set()
while pending:
    name = pending.pop()
    if name in copied:
        continue
    if "/" in name or not (source / name).is_file():
        raise SystemExit(f"Missing or unsupported IINA dependency: {name}")
    binary = source / name
    output = subprocess.check_output(["otool", "-L", str(binary)], text=True)
    for line in output.splitlines()[1:]:
        if " (compatibility version " not in line:
            continue
        dependency = line.strip().split(" (", 1)[0]
        if dependency.startswith("@rpath/"):
            pending.append(dependency.removeprefix("@rpath/"))
        elif not dependency.startswith(("/usr/lib/", "/System/Library/")):
            raise SystemExit(f"Unsupported dependency in {name}: {dependency}")
    shutil.copy2(binary, destination / name, follow_symlinks=True)
    copied.add(name)
print(f"Bundled {len(copied)} IINA libmpv libraries")
