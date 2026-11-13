#!/usr/bin/env python3
"""Compile the API-facing adapter against its exact fork source pin.

HORIZON_DIR may name an existing clean checkout. Cargo arguments are forwarded.
The temporary development manifest uses local SDK paths; the shipped manifest
and registry lock are never rewritten. This does not compile native callbacks.
"""
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "mixins/meridian"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def main():
    native = tomllib.loads((BUNDLE / "horizon-mod.toml").read_text())["native"]
    revision = native["horizon"]
    cache = pathlib.Path(os.environ.get("XDG_CACHE_HOME", pathlib.Path.home() / ".cache")) / "meridian"
    horizon = pathlib.Path(os.environ.get("HORIZON_DIR", cache / f"arismoko-{revision}"))
    if not (horizon / ".git").exists():
        horizon.mkdir(parents=True, exist_ok=True)
        run("git", "init", "-q", str(horizon))
        run("git", "-C", str(horizon), "remote", "add", "origin", "https://github.com/arismoko/horizon.git")
        run("git", "-C", str(horizon), "fetch", "--depth", "1", "origin", revision)
        run("git", "-C", str(horizon), "checkout", "--detach", "FETCH_HEAD")
    head = subprocess.check_output(["git", "-C", str(horizon), "rev-parse", "HEAD"], text=True).strip()
    if head != revision:
        sys.exit(f"Horizon is at {head}; this bundle pins {revision}")
    if subprocess.check_output(["git", "-C", str(horizon), "status", "--porcelain", "--untracked-files=no"], text=True):
        sys.exit("Horizon has tracked changes; use a clean pinned checkout")
    source = (BUNDLE / "Cargo.toml").read_text()
    source = re.sub(r"^horizon-mod-api = .*", f'horizon-mod-api = {{ path = "{horizon / "crates/horizon-mod-api"}" }}', source, flags=re.M)
    source = source.replace('path = "src/', f'path = "{BUNDLE}/src/')
    with tempfile.TemporaryDirectory(prefix="meridian-fork-api-") as temporary:
        stage = pathlib.Path(temporary)
        (stage / "Cargo.toml").write_text(source + "\n[workspace]\n")
        shutil.copyfile(horizon / "Cargo.lock", stage / "Cargo.lock")
        env = os.environ.copy()
        env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target/fork-api"))
        run("cargo", "check", "--manifest-path", str(stage / "Cargo.toml"), *sys.argv[1:], env=env)
    print(f"API adapter OK against arismoko/horizon {revision}; native callbacks require a launcher build")


if __name__ == "__main__":
    main()
