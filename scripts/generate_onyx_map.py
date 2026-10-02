#!/usr/bin/env python3
"""
Generates data/onyx_osc_map.json from Obsidian Onyx's official
"OSC Mapping v1.20 Revision 5" document.

Re-run this whenever the target list needs to change:
    python3 scripts/generate_onyx_map.py
"""
import json
import os

targets = []


def add(category, name, address, kind, value_range=None, feedback_address=None):
    targets.append({
        "category": category,
        "name": name,
        "address": address,
        "kind": kind,              # "fader" | "button" | "label"
        "range": value_range,      # e.g. [0, 255] for faders
        "feedback_address": feedback_address,
    })


# ---- Playbacks 1-20 ----
def playback_base(n: int) -> int:
    if 1 <= n <= 10:
        return 4201 + (n - 1) * 10
    if 11 <= n <= 20:
        return 4601 + (n - 11) * 10
    raise ValueError(n)


for n in range(1, 21):
    base = playback_base(n)
    pfa, pfb, fader, pfc, pfd = base, base + 1, base + 2, base + 3, base + 4
    cat = f"Playback {n}"
    add(cat, f"Playback {n} Fader (Level)", f"/Mx/fader/{fader}", "fader", [0, 255],
        feedback_address=f"/Mx/fader/{fader}")
    add(cat, f"Playback {n} PFA", f"/Mx/button/{pfa}", "button", [0, 1],
        feedback_address=f"/Mx/button/{pfa}/led")
    add(cat, f"Playback {n} PFB", f"/Mx/button/{pfb}", "button", [0, 1],
        feedback_address=f"/Mx/button/{pfb}/led")
    add(cat, f"Playback {n} PFC", f"/Mx/button/{pfc}", "button", [0, 1],
        feedback_address=f"/Mx/button/{pfc}/led")
    add(cat, f"Playback {n} PFD", f"/Mx/button/{pfd}", "button", [0, 1],
        feedback_address=f"/Mx/button/{pfd}/led")

# ---- Master faders ----
add("Masters", "Grand Master Level", "/Mx/fader/2202", "fader", [0, 255])
add("Masters", "Grand Master Flash", "/Mx/button/2201", "button", [0, 1],
    feedback_address="/Mx/button/2201/led")
add("Masters", "Flash Master Level", "/Mx/fader/2212", "fader", [0, 255])
add("Masters", "Flash Master Flash", "/Mx/button/2211", "button", [0, 1],
    feedback_address="/Mx/button/2211/led")
add("Masters", "Group Master A Level", "/Mx/fader/2222", "fader", [0, 255])
add("Masters", "Group Master A Flash", "/Mx/button/2221", "button", [0, 1],
    feedback_address="/Mx/button/2221/led")
add("Masters", "Group Master B Level", "/Mx/fader/2232", "fader", [0, 255])
add("Masters", "Group Master B Flash", "/Mx/button/2231", "button", [0, 1],
    feedback_address="/Mx/button/2231/led")

# ---- Transport ----
transport = {
    "SELECT": 5502,
    "RELEASE": 5503,
    "BEAT": 5504,
    "SNAP": 5511,
    "Pause/Back": 5512,
    "GO": 5513,
}
for name, addr in transport.items():
    add("Transport", name, f"/Mx/button/{addr}", "button", [0, 1],
        feedback_address=f"/Mx/button/{addr}/led")

# ---- Bank paging ----
add("Bank Paging", "Bank Page Up", "/Mx/button/4412", "button", [0, 1],
    feedback_address="/Mx/button/4412/led")
add("Bank Paging", "Bank Page Down", "/Mx/button/4413", "button", [0, 1],
    feedback_address="/Mx/button/4413/led")
for i in range(1, 6):
    addr = 4420 + i
    add("Bank Paging", f"Playback Bank {i} Select", f"/Mx/button/{addr}", "button", [0, 1])
add("Bank Paging", "Fader Swap (1-10 / 11-20)", "/Mx/button/4600", "button", [0, 1])

os.makedirs("data", exist_ok=True)
out_path = os.path.join("data", "onyx_osc_map.json")
with open(out_path, "w") as f:
    json.dump({"targets": targets}, f, indent=2)

print(f"Wrote {len(targets)} targets to {out_path}")
