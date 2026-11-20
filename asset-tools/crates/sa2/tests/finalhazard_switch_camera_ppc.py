"""Compare the Rust handoff camera packet with installed PPC instructions.

Usage: python finalhazard_switch_camera_ppc.py SA2_ASSETS_EXE Boss_Last2D.rel
Runs section 1 + 0xf5cc..0xf6a8 (handoff entering state 2) and
0xf724..0xf7fc (entering state 3). Relocated addresses are patched to a
controlled memory map; engine calls are observed and answered.
"""
import itertools
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

exe, module = sys.argv[1:]
data = Path(module).read_bytes()
word = lambda i: struct.unpack_from(">I", data, i)[0]
section = lambda n: word(word(16) + 8 * n) & ~1
code = section(1)

# Section 1 relocations, keyed by patched offset.
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

PLAYERS1, PLAYERS2, RESOURCES, CONSTANTS = 0x20000, 0x23000, 0x26000, 0x27000
GETMODE, RESET, SUBMIT = 0x800C3974, 0x800C3518, 0x800CB438
targets = {(0, 7, 0x801E7368): PLAYERS1, (0, 7, 0x801E7328): PLAYERS2,
           (0, 7, 0x801E5928): RESOURCES}

cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
cpu.mem_map(0, 0x30000)
# The packet scalar is copied with lfs/stfs, so enable the FPU (MSR[FP]).
cpu.reg_write(R.UC_PPC_REG_MSR, cpu.reg_read(R.UC_PPC_REG_MSR) | 0x2000)
cpu.mem_write(0xf5cc, data[code + 0xf5cc:code + 0xf800])
calls = {}
for at, (t, mod, s, add) in relocs.items():
    if not 0xf5cc <= at < 0xf800:
        continue
    if t == 10:
        calls[at] = add
        continue
    if mod == 88:
        assert s == 4, (hex(at), s)
        dest = CONSTANTS + add
        cpu.mem_write(dest, data[section(4) + add:section(4) + add + 4])
    else:
        dest = targets[(mod, s, add)]
    half = ((dest + 0x8000) >> 16) if t == 6 else dest & 0xFFFF
    assert t in (4, 6), (hex(at), t)
    cpu.mem_write(at, struct.pack(">H", half & 0xFFFF))

for site, fn in [(0xf680, GETMODE), (0xf690, GETMODE), (0xf69c, RESET), (0xf6a8, SUBMIT),
                 (0xf7d4, GETMODE), (0xf7e4, GETMODE), (0xf7f0, RESET), (0xf7fc, SUBMIT)]:
    assert calls.get(site) == fn, (hex(site), calls.get(site))

TASK = 0x28000
for slot in range(2):
    cpu.mem_write(PLAYERS1 + 4 * slot, struct.pack(">I", 0x21000 + 0x800 * slot))
    cpu.mem_write(PLAYERS2 + 4 * slot, struct.pack(">I", 0x24000 + 0x800 * slot))
offsets = (0x354, 0x364, 0x37C, 0x39C)
table = [0x11110000 + o for o in offsets]
for o, v in zip(offsets, table):
    cpu.mem_write(RESOURCES + o, struct.pack(">I", v))

def hook(u, pc, size, _):
    if pc not in calls:
        return
    if calls[pc] == GETMODE:
        assert u.reg_read(R.UC_PPC_REG_3) == 0
        first = not any(e[0] == "mode" for e in events)
        events.append(("mode", pc))
        u.reg_write(R.UC_PPC_REG_3, (mode if first else second) & 0xFFFFFFFF)
    elif calls[pc] == RESET:
        events.append(("reset", u.reg_read(R.UC_PPC_REG_3), u.reg_read(R.UC_PPC_REG_4)))
    elif calls[pc] == SUBMIT:
        events.append(("submit", u.reg_read(R.UC_PPC_REG_3),
                       list(struct.unpack(">6I", u.mem_read(u.reg_read(R.UC_PPC_REG_4), 24)))))
        u.emu_stop()
        return
    u.reg_write(R.UC_PPC_REG_PC, pc + 4)

cpu.hook_add(UC_HOOK_CODE, hook)
Path(".local").mkdir(exist_ok=True)
count = 0
with tempfile.TemporaryDirectory(dir=".local") as directory:
    p = Path(directory)
    for state, player, character, yaw, mode in itertools.product(
            (2, 3), (0, 1), (9, 10, -1), (0, 0x4000, 0x8000, 0x9000, -1, 0x7FFFFFFF, -0x80000000),
            (-1, 13, 14, 20)):
        second = 0x2A + mode
        events = []
        p1 = 0x21000 + 0x800 * player
        cpu.mem_write(TASK, bytes([player, state]))
        cpu.mem_write(p1 + 0xC, struct.pack(">i", yaw))
        cpu.mem_write(0x24000 + 0x800 * player + 1, struct.pack(">b", character))
        cpu.mem_write(0x1000, bytes(0x100))
        cpu.reg_write(R.UC_PPC_REG_1, 0x1000)
        cpu.reg_write(R.UC_PPC_REG_30, TASK)
        if state == 2:
            # 0xf5a8..0xf5ac leave the player-1 table in r4; r16 holds player 2.
            cpu.reg_write(R.UC_PPC_REG_4, PLAYERS1)
            cpu.reg_write(R.UC_PPC_REG_16, PLAYERS2)
            start = 0xf5cc
        else:
            cpu.reg_write(R.UC_PPC_REG_17, PLAYERS1)
            cpu.reg_write(R.UC_PPC_REG_16, PLAYERS2)
            cpu.reg_write(R.UC_PPC_REG_18, 0)
            start = 0xf724
        cpu.emu_start(start, 0xf800, count=200)
        assert events[-1][0] == "submit", events
        inp = dict(path="switch_camera", entered_state=state, player=player, character=character,
                   yaw=yaw, position_pointer=p1 + 0x14, table_words=table, camera_mode=mode,
                   second_lookup=second if mode > 13 else None)
        q = p / "request.json"
        out = p / f"{count}.json"
        q.write_text(json.dumps(inp))
        subprocess.run([exe, "finalhazard-placement-clock", module, out, q], check=True,
                       stdout=subprocess.DEVNULL)
        got = json.loads(out.read_text())
        resets = [e for e in events if e[0] == "reset"]
        sites = (0xf680, 0xf690) if state == 2 else (0xf7d4, 0xf7e4)
        assert [e for e in events if e[0] == "mode"] == [
            ("mode", s) for s in sites[:2 if mode > 13 else 1]], events
        expect = dict(camera_slot=events[-1][1], player=player, packet_words=events[-1][2],
                      reset_argument=resets[0][2] if resets else None,
                      reset_address=RESET if resets else None,
                      submit_address=SUBMIT)
        assert {k: got[k] for k in expect} == expect, (inp, got, events)
        assert not resets or resets[0][1] == 0, resets
        assert table[offsets.index(got["resource_table_offset"])] == events[-1][2][2], got
        count += 1
print(f"{count} installed PPC handoff camera comparisons passed")
