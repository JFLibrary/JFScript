#!/usr/bin/env python3
"""linux_test.py -- the tests on the Linux programs, from Windows through WSL

    python tools/linux_test.py

`cargo jflib test -p jfs` and `cargo jflib test -p scripttest` run the programs of the machine they
run on.  On Windows this script builds the Linux programs (cargo build --target
x86_64-unknown-linux-gnu) and runs both tests as `cargo jflib test` does, in the package's folder,
through the WSL distribution JFLIB_WSL_DISTRO (default Ubuntu-24.04).  On Linux:
`cargo jflib test -p jfs --target x86_64-unknown-linux-gnu`, the same for scripttest.
"""
import os
import shlex
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRIPLE = "x86_64-unknown-linux-gnu"
DISTRO = os.environ.get("JFLIB_WSL_DISTRO", "Ubuntu-24.04")


def wsl_path(p):
    p = str(Path(p).resolve())
    return "/mnt/" + p[0].lower() + p[2:].replace("\\", "/")


def main():
    if os.name != "nt":
        sys.exit("linux_test.py runs on Windows; on Linux: cargo jflib test -p jfs --target " + TRIPLE)
    r = subprocess.run(["cargo", "build", "--workspace", "--target", TRIPLE], cwd=ROOT)
    if r.returncode:
        sys.exit(r.returncode)
    failed = total = 0
    for pkg in ["jfs", "scripttest"]:
        exe = wsl_path(ROOT / "target" / TRIPLE / "debug" / pkg)
        for s in sorted((ROOT / pkg / "tests").glob("*.uis")):
            total += 1
            first = s.read_text(encoding="utf-8").splitlines()[:1]
            extra = shlex.split(first[0][len("#!args"):]) if first and first[0].startswith("#!args") else []
            rel = s.relative_to(ROOT / pkg).as_posix()
            p = subprocess.run(["wsl.exe", "-d", DISTRO, "--cd", wsl_path(ROOT / pkg), "-e", exe, *extra, "--script", rel],
                               stdin=subprocess.DEVNULL, capture_output=True, text=True, encoding="utf-8", errors="replace")
            lines = (p.stdout + p.stderr).strip().splitlines()
            if p.returncode == 0:
                print(f"ok   {pkg}/{rel}: {lines[-1] if lines else ''}")
                continue
            failed += 1
            print(f"FAIL {pkg}/{rel} (exit {p.returncode})")
            for line in lines:
                if line.lstrip().startswith(("FAIL", "error", "panic")):
                    print("     " + line.strip())
    print(f"{total - failed} of {total} scripts passed on Linux ({DISTRO})")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
