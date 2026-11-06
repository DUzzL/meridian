#!/usr/bin/env python3
"""Converts the engine's tables/btree21wd.h (the 1.21.4+ biome parameter
R-tree) to Rust. Usage: btree_table.py <btree21wd.h> <out.rs>"""
import re
import sys

s = open(sys.argv[1]).read()
s = re.sub(r"//[^\n]*", "", s)
s = re.sub(r"/\*.*?\*/", "", s, flags=re.S)
order = int(re.search(r"btree21wd_order = (\d+)", s).group(1))
steps = [int(x) for x in re.search(r"btree21wd_steps\[\] = \{([^}]*)\}", s).group(1).replace(" ", "").split(",") if x]
pm = s[s.index("btree21wd_param"):]
pm = pm[pm.index("{") + 1:pm.index("};")]
pairs = re.findall(r"\{\s*(-?\d+)\s*,\s*(-?\d+)\s*\}", pm)
nd = s[s.index("btree21wd_nodes"):]
nd = nd[nd.index("{") + 1:nd.index("};")]
nodes = re.findall(r"0x[0-9A-Fa-f]+", nd)
with open(sys.argv[2], "w") as o:
    o.write("// Generated from the engine's tables/btree21wd.h (the 1.21.4+ biome\n// parameter R-tree) by tools/seedatlas-fixtures/btree_table.py; do not edit by hand.\n\n")
    o.write(f"pub const ORDER: u32 = {order};\npub const STEPS: [u32; {len(steps)}] = {steps};\n")
    o.write(f"pub static PARAM: [[i32; 2]; {len(pairs)}] = [\n")
    for i in range(0, len(pairs), 6):
        o.write("    " + " ".join(f"[{a}, {b}]," for a, b in pairs[i:i + 6]) + "\n")
    o.write(f"];\npub static NODES: [u64; {len(nodes)}] = [\n")
    for i in range(0, len(nodes), 6):
        o.write("    " + " ".join(f"{n}," for n in nodes[i:i + 6]) + "\n")
    o.write("];\n")
