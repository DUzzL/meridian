#!/usr/bin/env python3
"""Compile Meridian's adapter against the SDK of the build it pins.

`cargo horizon-mod` keeps the pinned SDK in `mixins/meridian/.horizon/sdk/`
(fetched and checked by any of its commands); this script fetches it when it
is missing, then checks a staged copy of the bundle so that the shipped
manifest and registry lock are never rewritten. Cargo arguments are
forwarded (`--all-targets`, `clippy` via MERIDIAN_CARGO_COMMAND=clippy).
The mod uses the public mod API only, so this compiles everything the
launcher compiles; the launcher's build stays the final check.
"""
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "mixins/meridian"
SDK = BUNDLE / ".horizon/sdk"


def main():
    pin = tomllib.loads((BUNDLE / "horizon-mod.toml").read_text())["native"]["horizon"]
    build = SDK / "build"
    if not build.exists() or build.read_text().strip() != pin:
        tool = os.environ.get("HORIZON_MOD_TOOL", "cargo-horizon-mod")
        # `hooks` fetches the pinned SDK without running Cargo on the bundle.
        subprocess.run([tool, "horizon-mod", "hooks", "render_frame_pre"], cwd=BUNDLE, check=True,
                       stdout=subprocess.DEVNULL)
    if build.read_text().strip() != pin:
        sys.exit(f"{SDK} holds {build.read_text().strip()}, the bundle pins {pin}")
    api = SDK / "api/crates/horizon-mod-api"
    source = (BUNDLE / "Cargo.toml").read_text()
    source = source.replace('path = ".horizon/sdk/api/crates/horizon-mod-api"', f'path = "{api}"')
    source = source.replace('path = "src/', f'path = "{BUNDLE}/src/')
    command = os.environ.get("MERIDIAN_CARGO_COMMAND", "check")
    with tempfile.TemporaryDirectory(prefix="meridian-api-") as temporary:
        stage = pathlib.Path(temporary)
        (stage / "Cargo.toml").write_text(source + "\n[workspace]\n")
        shutil.copyfile(BUNDLE / "Cargo.lock", stage / "Cargo.lock")
        env = os.environ.copy()
        env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target/api-check"))
        subprocess.run(["cargo", command, "--manifest-path", str(stage / "Cargo.toml"), *sys.argv[1:]],
                       env=env, check=True)
    print(f"Adapter OK against the mod API of horizon-rust/horizon {pin}")


if __name__ == "__main__":
    main()
