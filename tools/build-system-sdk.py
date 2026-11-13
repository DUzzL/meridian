#!/usr/bin/env python3
"""Builds a local macOS mod SDK with the game's `system-allocator` feature.

The published arm64 SDK at the pin mixes allocators across its dylibs and
aborts before a world opens (arismoko/horizon#3877, fixed in source after the
pin). This reuses the fork's own `tools/prebuilt_game.py` from the clean
pinned checkout and only adds that feature for darwin. Test aid, not a fork
change.

usage: build-system-sdk.py HORIZON_DIR HORIZON_STAGE OUT_DIR
  HORIZON_DIR    clean checkout of arismoko/horizon at the pinned commit
  HORIZON_STAGE  the horizon-stage tool built from that checkout
  OUT_DIR        where the SDK goes (HORIZON_LAUNCHER_DEVELOPER_SDK)
"""
import importlib.util
import pathlib
import sys

if len(sys.argv) != 4:
    sys.exit(__doc__)
source, stage_tool, out = (pathlib.Path(a).resolve() for a in sys.argv[1:])
script = source / "tools" / "prebuilt_game.py"
sys.path.insert(0, str(script.parent))
spec = importlib.util.spec_from_file_location("prebuilt_game", script)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.PROFILE = "fast"
original_stage = module.stage
module.stage = lambda target_dir: original_stage(target_dir, stage_tool)
original_features = module.cargo_features
module.cargo_features = lambda server, platform: original_features(server, platform) + (
    ["--features", "system-allocator"] if platform == "darwin" else []
)
module.build(out, False)
