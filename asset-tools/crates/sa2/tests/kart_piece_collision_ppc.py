"""Check `kart-piece-collision` against CartD's piece collision tasks under Unicorn.
Usage: python kart_piece_collision_ppc.py CLI EXE DATA_DLL CartD.rel PRIVATE_DIR

* +0x65f0 builds the task's collision object from the live entry: run on the
  GameCube course rows for every tasked occurrence of all five courses
  (live entries filled from `kart-native-layout`), compare flags, position,
  rotation and scale with Rust.
* +0x64b8 decides each frame whether the collision stays registered: run on
  generated slot/mode/count cases and compare the register/unregister call.
"""
import json
from gsne8p import steam_rel
import random
import struct
import subprocess
import sys
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

cli, exe, dll, rel, work = map(Path, sys.argv[1:])
work.mkdir(parents=True, exist_ok=True)
for old in work.glob("*.json"):
    old.unlink()  # the CLI never overwrites an existing report
data = rel.read_bytes()
word = lambda at: struct.unpack_from(">I", data, at)[0]
base = {1: 0x81000000, 4: 0x83000000, 5: 0x82000000, 6: 0x84000000}
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
for section, addr in base.items():
    row = word(16) + 8 * section
    off, size = word(row) & ~1, word(row + 4)
    u.mem_map(addr, (size + 4095) & ~4095)
    if off:
        u.mem_write(addr, data[off:off + size])
for mod, at in struct.iter_unpack(">II", data[word(40):word(40) + word(44)]):
    while True:
        delta, kind, sec, add = steam_rel(struct.unpack_from(">HBBI", data, at), mod)
        at += 8
        if kind == 203:
            break
        if kind == 202:
            target, loc = sec, 0
            continue
        loc += delta
        if mod != 80 or sec not in base:
            continue
        value = base[sec] + add
        if target == 1 and kind in (4, 6):
            half = (value + 0x8000) >> 16 if kind == 6 else value & 0xFFFF
            u.mem_write(base[1] + loc, struct.pack(">H", half))
        elif target == 5 and kind == 1:
            u.mem_write(base[5] + loc, struct.pack(">I", value))
reg = lambda n: getattr(R, "UC_PPC_REG_" + str(n))
ram, stack = 0x85000000, 0x86000000
u.mem_map(ram, 0x40000)
u.mem_map(stack, 0x10000)
u.reg_write(R.UC_PPC_REG_MSR, u.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
put = lambda a, fmt, *v: u.mem_write(a, struct.pack(">" + fmt, *v))
get = lambda a, fmt: struct.unpack(">" + fmt, u.mem_read(a, struct.calcsize(">" + fmt)))
calls = []
def toggle(machine, addr, size, user):
    calls.append(machine.reg_read(reg(4)))
    machine.reg_write(R.UC_PPC_REG_PC, machine.reg_read(R.UC_PPC_REG_LR))
u.hook_add(UC_HOOK_CODE, toggle, begin=base[1] + 0x6734, end=base[1] + 0x6734)
def alloc(machine, addr, size, user):
    machine.reg_write(reg(3), ram + 0x30000)
    machine.reg_write(R.UC_PPC_REG_PC, addr + 4)
u.hook_add(UC_HOOK_CODE, alloc, begin=base[1] + 0x660C, end=base[1] + 0x660C)
def run(start, end, r3):
    u.reg_write(reg(1), stack + 0x8000)
    u.reg_write(reg(3), r3)
    u.reg_write(R.UC_PPC_REG_LR, base[1] + end)
    u.emu_start(base[1] + start, base[1] + end, count=100000)
    assert u.reg_read(R.UC_PPC_REG_PC) == base[1] + end

S6 = base[6]
TASK, TWP = ram, ram + 0x100
put(TASK + 0x34, "I", TWP)

# --- +0x64b8 activation -------------------------------------------------
rng = random.Random(7)
cases = []
for _ in range(400):
    count = rng.choice([88, 112, 123, 321, 369, rng.randrange(1, 400)])
    occ = rng.choice([0, 4, 5, 6, count - 31, count - 30, count - 6, count - 5, count - 4,
                      rng.randrange(0, max(1, count))]) % 65536
    owners = []
    for slot in range(6):
        if rng.random() < 0.15:
            owners.append(None)
        else:
            owners.append(rng.choice([occ - 11, occ - 10, occ - 9, occ + 3, occ + 4, occ + 5,
                                      rng.randrange(-5, count + 5)]))
    cases.append(dict(occurrence=occ, count=count, raceMode=rng.choice([-1, 0, 1, 2, 3]),
                      kartOwners=owners))
native = []
for c in cases:
    put(base[5] + 0x3DE0, "I", 0)
    put(S6 + 4 + 0x19C, "i", c["raceMode"])
    put(S6 + 0x36DB8, "i", c["count"])
    put(TWP + 6, "H", c["occurrence"])
    for slot, owner in enumerate(c["kartOwners"]):
        kart = ram + 0x1000 + 0x100 * slot
        put(S6 + 0x21428 + 4 * slot, "I", 0 if owner is None else kart)
        put(kart + 0x38, "I", kart + 0x40)
        put(kart + 0x40 + 0xBC, "I", slot)
        put(S6 + 0x24D84 + 4 * slot, "i", 0 if owner is None else owner)
    calls.clear()
    run(0x64B8, 0x65E0, TASK)
    assert len(calls) == 1, calls
    native.append(bool(calls[0]))
cases_path = work / "cases.json"
cases_path.write_text(json.dumps(cases))

layout = {}
for name in ("101", "280", "beginner", "standard", "expert"):
    out = work / f"pieces-{name}.json"
    subprocess.run([str(cli), "kart-piece-collision", str(exe), str(out), str(dll), name,
                    str(cases_path)], check=True)
    report = json.loads(out.read_text())
    assert report["activation"] == native, name
    lay = work / f"layout-{name}.json"
    subprocess.run([str(cli), "kart-native-layout", str(exe), str(lay), str(dll), name], check=True)
    placements = json.loads(lay.read_text())
    # --- +0x65f0 object copy --------------------------------------------
    for piece in report["pieces"]:
        p = placements[piece["occurrence"]]
        entry = S6 + 0x36DC0 + 0x30 * piece["occurrence"]
        gc_row = base[5] + 0xF9EDC + 108 * piece["pieceId"]
        put(entry + 4, "fff", *p["position"])
        put(entry + 0x14, "I", get(gc_row + 4, "I")[0])
        put(entry + 0x18, "i", p["yaw"])
        put(TWP + 6, "H", piece["occurrence"])
        u.mem_write(ram + 0x30000, b"\xCC" * 0x38)
        run(0x65F0, 0x66DC, TASK)
        f32bits = lambda v: list(struct.unpack(">3I", struct.pack(">3f", *v)))
        flags, = get(ram + 0x30000, "I")
        pos = list(get(ram + 0x30008, "3I"))
        rot = list(get(ram + 0x30014, "iii"))
        scale = list(get(ram + 0x30020, "3I"))
        assert get(TASK + 0x38, "I")[0] == ram + 0x30000
        assert (flags, pos, rot, scale) == (piece["flags"], f32bits(piece["position"]),
                                            piece["rotation"], f32bits(piece["scale"])), piece
    print(name, "tasked pieces", len(report["pieces"]), "activation cases", len(native),
          "active", sum(native))
