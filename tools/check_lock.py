#!/usr/bin/env python3
"""Checks that a Cargo.lock only uses crates (name and version) that Horizon's
Cargo.lock has: a native mod may not bring crates of its own.

    tools/check_lock.py                         the repo's Cargo.lock against the pinned Horizon
    tools/check_lock.py --against H.lock A.lock [B.lock ...]
"""
import argparse
import pathlib
import re
import sys
import tomllib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
MIXIN = ROOT / "mixins" / "meridian" / "horizon-mixin.toml"


def packages(text):
    """(name, version) of every package that comes from a registry or git."""
    lock = tomllib.loads(text)
    return {(p["name"], p["version"]) for p in lock.get("package", []) if "source" in p}


def pinned_lock():
    rev = re.search(r'^horizon_revision = "([0-9a-f]{40})"', MIXIN.read_text(), re.M).group(1)
    url = f"https://raw.githubusercontent.com/horizon-rust/horizon/{rev}/Cargo.lock"
    with urllib.request.urlopen(url, timeout=60) as r:
        return r.read().decode()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--against", type=pathlib.Path, help="Horizon's Cargo.lock (default: download the pinned one)")
    ap.add_argument("locks", nargs="*", type=pathlib.Path, default=[ROOT / "Cargo.lock"])
    args = ap.parse_args()
    allowed = packages(args.against.read_text() if args.against else pinned_lock())
    bad = []
    for lock in args.locks:
        for name, version in sorted(packages(lock.read_text()) - allowed):
            # Horizon's own crates come from git in the adapter's lock.
            if name.startswith("horizon"):
                continue
            bad.append(f"{lock}: {name} {version}")
    if bad:
        print("crates Horizon does not lock:", *bad, sep="\n  ")
        return 1
    print("lockfiles use only crates Horizon locks")
    return 0


if __name__ == "__main__":
    sys.exit(main())
