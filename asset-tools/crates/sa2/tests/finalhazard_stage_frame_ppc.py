"""Compare the Rust voice gate and arena clamp with installed PPC instructions.

Usage: python finalhazard_stage_frame_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs Boss_Last2D.rel section 1 + 0xeb38 (voice-channel gate) and + 0xee8c
(per-frame arena bounds, including the local square root at + 0x40cc) to
their returns. Engine imports are observed and answered.
"""
import itertools
import json
import math
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile

from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

exe, module = sys.argv[1:]
data = Path(module).read_bytes()
word = lambda i: struct.unpack_from(">I", data, i)[0]
section = lambda n: (word(word(16) + 8 * n) & ~1, word(word(16) + 8 * n + 4))
code, code_size = section(1)

relocs = {}
imp_off, imp_size = struct.unpack_from(">II", data, 0x28)
for i in range(imp_off, imp_off + imp_size, 8):
    mod, off = struct.unpack_from(">II", data, i)
    pos, sec = 0, None
    while True:
        o, t, s, add = struct.unpack_from(">HBBI", data, off)
        off += 8
        if t == 203:
            break
        pos += o
        if t == 202:
            sec, pos = s, 0
        elif t != 201 and sec == 1:
            relocs[pos] = (t, mod, s, add)

CODE, RETURN, STACK = 0x100000, 0x0FFFF0, 0x0F0000
cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x200000)
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
RANGES = [(0x40CC, 0x4158), (0xEB38, 0xEC04), (0xEE8C, 0xF070)]
windows, imports = {}, {}
for a, b in RANGES:
    cpu.mem_write(CODE + a, data[code + a:code + b])
for at, (t, mod, s, add) in sorted(relocs.items()):
    if not any(a <= at < b for a, b in RANGES):
        continue
    if t == 10:
        imports[at] = add
        continue
    assert t in (4, 6), (hex(at), t)
    key = (mod, s, add)
    if key not in windows:
        windows[key] = 0x40000 + 0x1000 * len(windows)
        start, size = section(s) if mod == 88 else (0, 0)
        if start:
            cpu.mem_write(windows[key], data[start + add:start + min(size, add + 0x1000)])
    dest = windows[key]
    half = ((dest + 0x8000) >> 16) if t == 6 else dest & 0xFFFF
    cpu.mem_write(CODE + at, struct.pack(">H", half & 0xFFFF))

VOICE = windows[(0, 9, 0x803AD3D0)]
WORKPTR = windows[(88, 6, 0x4A8)]
PRESENT, PLAYER1 = windows[(0, 7, 0x801E7388)], windows[(0, 7, 0x801E7368)]
CHANNELS, WORK, P1 = 0x80000, 0x81000, 0x82000
cpu.mem_write(WORKPTR, struct.pack(">I", WORK))
for s in range(2):
    cpu.mem_write(PLAYER1 + 4 * s, struct.pack(">I", P1 + 0x100 * s))
f2b = lambda f: struct.unpack(">I", struct.pack(">f", f))[0]
fpr = lambda u, n: struct.unpack(">d", struct.pack(">Q", u.reg_read(getattr(R, f"UC_PPC_REG_FPR{n}"))))[0]


def hook(u, pc, size, _):
    off = pc - CODE
    if pc == RETURN:
        u.emu_stop()
        return
    if off not in imports:
        return
    target = imports[off]
    if target in (0x8010AC70, 0x8010ACBC):  # save/restore FPR helpers
        pass
    elif target == 0x8011B314:  # distance(a, b)
        vec = lambda p: list(struct.unpack(">3f", u.mem_read(p, 12)))
        events.append(dict(site=off, call=target, vectors=[vec(u.reg_read(R.UC_PPC_REG_3)),
                                                            vec(u.reg_read(R.UC_PPC_REG_4))]))
        slot = u.reg_read(R.UC_PPC_REG_28)  # loop slot (0xeec4 / 0xf044)
        bits = struct.unpack(">Q", struct.pack(">d", distances[slot]))[0]
        u.reg_write(R.UC_PPC_REG_FPR1, bits)
    elif target == 0x80039754:  # set position(slot, x, y, z)
        events.append(dict(site=off, call=target, args=[u.reg_read(R.UC_PPC_REG_3)],
                           floats=[fpr(u, n) for n in (1, 2, 3)]))
    elif target == 0x8001162C:  # stop voice channel
        events.append(dict(site=off, call=target, args=[u.reg_read(R.UC_PPC_REG_3)]))
    else:
        raise AssertionError(f"unexpected import {target:#x} at {off:#x}")
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)


cpu.hook_add(UC_HOOK_CODE, hook)


def call(entry):
    cpu.reg_write(R.UC_PPC_REG_1, STACK)
    cpu.reg_write(R.UC_PPC_REG_LR, RETURN)
    cpu.emu_start(CODE + entry, RETURN + 4, count=5000)
    return cpu.reg_read(R.UC_PPC_REG_3)


def rust(p, n, request):
    q = p / "request.json"
    out = p / f"{n}.json"
    q.write_text(json.dumps(request))
    subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                   stdout=subprocess.DEVNULL)
    return json.loads(out.read_text())


Path(".local").mkdir(exist_ok=True)
gate_cases = arena_cases = clamps = 0
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    # Voice gate: null table plus every pair of channel states/categories.
    states = (-2, -1, 0, 1, 4, 5, 6)
    combos = [None] + list(itertools.product(states, (3, 2), states, (3, -1)))
    for n, combo in enumerate(combos):
        events = []
        if combo is None:
            cpu.mem_write(VOICE, struct.pack(">I", 0))
            channels = None
        else:
            cpu.mem_write(VOICE, struct.pack(">I", CHANNELS))
            channels = []
            for i in range(2):
                state, cat = combo[2 * i:2 * i + 2]
                cpu.mem_write(CHANNELS + 0x1C * i + 0x30, struct.pack(">i", state))
                cpu.mem_write(CHANNELS + 0x1C * i + 0x25, struct.pack(">b", cat))
                channels.append(dict(state=state, category=cat))
        result = struct.unpack(">i", struct.pack(">I", call(0xEB38) & 0xFFFFFFFF))[0]
        got = rust(p, n, dict(path="voice_gate", channels=channels))
        assert got == dict(result=result, events=events), (combo, got, events, result)
        gate_cases += 1

    # Arena clamp: positions inside/outside the radius, past x/y bounds.
    rng = random.Random(421)
    coords = (0.0, 100.0, 249.9, 250.0, 260.0, -1499.0, 1499.0, 1500.0, 1600.0, -3000.0)
    for n in range(600):
        switch_state = rng.choice((0, 1, 2, 3, 4, 4, -1))
        present = [rng.random() < 0.8 for _ in range(2)]
        positions = [[rng.choice(coords) + rng.uniform(-1, 1) * rng.choice((0, 1)),
                      rng.choice((0.0, 299.0, 300.0, 301.0, -499.0, -500.0, -501.0, 5000.0)),
                      rng.choice(coords) + rng.uniform(-1, 1) * rng.choice((0, 1))]
                     for _ in range(2)]
        positions = [[struct.unpack(">f", struct.pack(">f", v))[0] for v in pos] for pos in positions]
        distances = []
        for pos in positions:
            d = math.hypot(pos[0], pos[2])
            # Engine distance is external: sometimes disagree with geometry.
            d = rng.choice((d, d, 1500.0, 1499.9, 2000.0, 0.0))
            distances.append(struct.unpack(">f", struct.pack(">f", d))[0])
        events = []
        cpu.mem_write(WORK, bytes([0, switch_state & 0xFF]))
        for s in range(2):
            cpu.mem_write(PRESENT + 4 * s, struct.pack(">I", 0x9000 if present[s] else 0))
            cpu.mem_write(P1 + 0x100 * s + 0x14, struct.pack(">3f", *positions[s]))
        call(0xEE8C)
        got = rust(p, 1000 + n, dict(path="arena_clamp", switch_state=switch_state,
                                     present=present, positions=positions, distances=distances))
        assert got["skipped"] == (switch_state == 4)
        rev = [{k: v for k, v in e.items() if k != "radial_clamp"} for e in got["events"]]
        assert rev == events, (n, got, events)
        clamps += sum(e.get("radial_clamp", False) for e in got["events"])
        arena_cases += 1
assert clamps > 50, clamps
print(f"{gate_cases} voice gate and {arena_cases} arena clamp PPC comparisons passed "
      f"({clamps} radial clamps)")
