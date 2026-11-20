"""Compare a private crossing-directory report with SA Tools splits and CartD instructions.
Usage: python kart_crossing_ppc.py CartD.rel crossings.json Courses/
"""
import json
from gsne8p import steam_rel
import struct
import sys
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

rel, report, splits = map(Path, sys.argv[1:])
data = rel.read_bytes()
doc = json.loads(report.read_text())
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
word = lambda at: struct.unpack_from(">I", data, at)[0]
base = {1: 0x81000000, 5: 0x82000000}
for section, addr in base.items():
    row = word(16) + 8 * section
    off, size = word(row) & ~1, word(row + 4)
    u.mem_map(addr, (size + 4095) & ~4095)
    u.mem_write(addr, data[off:off + size])
for mod, at in struct.iter_unpack(">II", data[word(40):word(40)+word(44)]):
    while True:
        delta, kind, sec, add = steam_rel(struct.unpack_from(">HBBI", data, at), mod)
        at += 8
        if kind == 203: break
        if kind == 202:
            target, loc = sec, 0
            continue
        loc += delta
        if mod == 80 and target == 1 and loc == 0x715a:
            directory = base[sec] + add
        if mod == 80 and target == 5 and kind == 1 and sec in base:
            u.mem_write(base[5] + loc, struct.pack(">I", base[sec] + add))
ram = 0x85000000
u.mem_map(ram, 0x20000)
reg = lambda n: getattr(R, "UC_PPC_REG_" + str(n))
def observed(machine, addr, size, user):
    src, dest = (machine.reg_read(reg(n)) for n in (3, 6))
    assert dest == ram + 0x8000 + 32 * len(calls)
    calls.append(bytes(machine.mem_read(src, 32)))
    machine.reg_write(R.UC_PPC_REG_PC, machine.reg_read(R.UC_PPC_REG_LR))
u.hook_add(UC_HOOK_CODE, observed, begin=base[1]+0x74ac, end=base[1]+0x74ac)
for course in doc["courses"]:
    name = course["course"]
    filename = ("Route " + name if name.isdigit() else name.title()) + ".txt"
    ids = bytes(map(int, (splits / filename).read_text().split()))
    assert list(ids) == course["piece_ids"]
    u.mem_write(ram, ids)
    u.mem_write(ram + 0x1000, bytes(4))
    calls = []
    for i in range(len(ids)):
        for n, v in {14:ram, 16:ram+0x2000, 17:ram+0x8000, 26:i, 27:directory,
                     28:ram+0x1100, 30:ram+0x1000}.items():
            u.reg_write(reg(n), v)
        u.emu_start(base[1]+0x7388, base[1]+0x7428, count=4096)
        assert u.reg_read(R.UC_PPC_REG_PC) == base[1]+0x7428
    count, = struct.unpack(">I", u.mem_read(ram+0x1000, 4))
    rows = course["segment_occurrence_and_local_index"]
    assert count == course["segment_count"] == len(rows) == len(calls)
    owners = struct.unpack(f">{count}I", u.mem_read(ram+0x2000, 4*count))
    assert len(course["segment_event_modes"])==count
    for idx,((occ, local), owner, raw) in enumerate(zip(rows, owners, calls)):
        assert occ == owner
        pid=ids[owner]; inside=50<=pid<=71
        assert course["segment_event_modes"][idx]==dict(piece_id=pid,engine_8006c97c_arg=0 if inside else -127,return_word=int(inside))
        quad = doc["piece_local_xz_quads"][ids[occ]][local]
        assert struct.pack(">8f", *(v for pair in quad for v in pair)) == raw
    print(name, len(ids), count)
