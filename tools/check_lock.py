#!/usr/bin/env python3
"""Check that workspace registry identities are pinned by the mod's own lock.

The fork permits mod-owned registry dependencies. This check compares identities
and checksums against the checked-in native registry lock, without changing it.
The launcher separately enforces that the prebuilt game's graph is immutable.
"""
import argparse
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
MOD_LOCK = ROOT / "mixins" / "meridian" / "Cargo.lock"


def packages(text):
    """(name, version) of every package that comes from a registry or git."""
    lock = tomllib.loads(text)
    return {(p["name"], p["version"], p.get("checksum")) for p in lock.get("package", []) if p.get("source", "").startswith("registry+")}



def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--against", type=pathlib.Path, default=MOD_LOCK, help="reference lock (default: the mod registry lock)")
    ap.add_argument("locks", nargs="*", type=pathlib.Path, default=[ROOT / "Cargo.lock"])
    args = ap.parse_args()
    allowed = packages(args.against.read_text())
    bad = []
    for lock in args.locks:
        for name, version, checksum in sorted(packages(lock.read_text()) - allowed):
            bad.append(f"{lock}: {name} {version}")
    if bad:
        print("registry identities absent from the reference lock:", *bad, sep="\n  ")
        return 1
    print("workspace registry identities and checksums match the mod registry lock")
    return 0


if __name__ == "__main__":
    sys.exit(main())
