#!/usr/bin/env python3
"""Converts one of the engine's biome parameter R-trees (tables/btree18.h,
btree192.h, btree19.h, btree20.h, btree21wd.h) to Rust.
Usage: btree_table.py <btreeNAME.h> <out.rs>   (NAME is taken from the file name)"""
import os
import re
import sys

name = os.path.splitext(os.path.basename(sys.argv[1]))[0]
s = open(sys.argv[1]).read()
s = re.sub(r"//[^\n]*", "", s)
s = re.sub(r"/\*.*?\*/", "", s, flags=re.S)
order = int(re.search(name + r"_order = (\d+)", s).group(1))
steps = [int(x) for x in re.search(name + r"_steps\[\] = \{([^}]*)\}", s).group(1).replace(" ", "").split(",") if x]
pm = s[s.index(name + "_param"):]
pm = pm[pm.index("{") + 1:pm.index("};")]
pairs = re.findall(r"\{\s*(-?\d+)\s*,\s*(-?\d+)\s*\}", pm)
nd = s[s.index(name + "_nodes"):]
nd = nd[nd.index("{") + 1:nd.index("};")]
nodes = re.findall(r"0x[0-9A-Fa-f]+", nd)
with open(sys.argv[2], "w") as o:
    o.write(f"// Generated from the engine's tables/{name}.h (a biome parameter\n// R-tree) by tools/seedatlas-fixtures/btree_table.py; do not edit by hand.\n\n")
    o.write(f"pub const ORDER: u32 = {order};\npub const STEPS: [u32; {len(steps)}] = {steps};\n")
    o.write(f"pub static PARAM: [[i32; 2]; {len(pairs)}] = [\n")
    for i in range(0, len(pairs), 6):
        o.write("    " + " ".join(f"[{a}, {b}]," for a, b in pairs[i:i + 6]) + "\n")
    o.write(f"];\npub static NODES: [u64; {len(nodes)}] = [\n")
    for i in range(0, len(nodes), 6):
        o.write("    " + " ".join(f"{n}," for n in nodes[i:i + 6]) + "\n")
    o.write("];\n")
