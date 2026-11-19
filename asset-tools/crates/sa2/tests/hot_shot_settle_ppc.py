"""Compare Rust with the installed Hot Shot height-settle arithmetic; run from the checkout root."""
import hashlib
import itertools
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

from unicorn import Uc, UC_ARCH_PPC, UC_MODE_BIG_ENDIAN, UC_MODE_PPC32
from unicorn import ppc_const as regs

game = Path(os.environ["SA2_GAME_PATH"])
module = game / "resource/gd_PC/boss_hotshotD.rel"
raw = module.read_bytes()
assert hashlib.sha256(raw).hexdigest() == "9e0caebeefbdeb2e2829f17510b44d9238956342b3eed10a2ae7e1ff56358960"
word = lambda off: struct.unpack_from(">I", raw, off)[0]
section = lambda n: word(word(16) + n * 8) & ~3
cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
code, data, work, entity = 0x81000000, 0x82000000, 0x200000, 0x300000
for addr in (code + 0x1000, data, work, entity):
    cpu.mem_map(addr, 4096)
# Enter immediately after the helpers; stop before the shared tail.
lo, hi = 0x13c8, 0x14c0
stop = code + 0x15a8
cpu.mem_write(code + lo, raw[section(1)+lo:section(1)+hi])
cpu.mem_write(data, raw[section(4):section(4)+4096])
for owner, at in struct.iter_unpack(">II", raw[word(40):word(40)+word(44)]):
    while True:
        delta, kind, sec, add = struct.unpack_from(">HBBI", raw, at)
        at += 8
        if kind == 203:
            break
        if kind == 202:
            source, off = sec, 0
            continue
        off += delta
        if source != 1 or not lo <= off < hi:
            continue
        assert (owner, sec) == (82, 4) and kind in (4, 6)
        address = data + add
        half = (address + 0x8000) >> 16 if kind == 6 else address
        cpu.mem_write(code + off, struct.pack(">H", half & 65535))

cpu.reg_write(regs.UC_PPC_REG_MSR, cpu.reg_read(regs.UC_PPC_REG_MSR) | 8192)
def neighbors(value):
    bits = struct.unpack(">I", struct.pack(">f", value))[0]
    return [struct.unpack(">f", struct.pack(">I", bits+i))[0] for i in (-1, 0, 1)]

ys = neighbors(9.95) + neighbors(10.05) + [-100., 10., 100.]
speeds = neighbors(-0.0625) + neighbors(0.0625) + [-0., 0.125]
local = Path.cwd() / ".local"
local.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=local) as tmp:
    for i, (y, speed) in enumerate(itertools.product(ys, speeds)):
        correction = 0.75 if i % 2 else -0.875
        for addr, value in [(entity+0x18, y), (work+4, speed), (work+0x10, correction)]:
            cpu.mem_write(addr, struct.pack(">f", value))
        cpu.mem_write(entity+1, bytes([3]))
        cpu.mem_write(entity+6, struct.pack(">H", 17))
        cpu.reg_write(regs.UC_PPC_REG_31, entity)
        cpu.reg_write(regs.UC_PPC_REG_29, work)
        cpu.emu_start(code+lo, stop, count=100)
        assert cpu.reg_read(regs.UC_PPC_REG_PC) == stop
        request = dict(y=y, work04=speed, work10=correction, angles=[0,0],
                       flags=1, timer=17, controller_state=0)
        req, out = Path(tmp)/f"{i}-req.json", Path(tmp)/f"{i}.json"
        req.write_text(json.dumps(request))
        subprocess.run([sys.argv[1], "hot-shot-settle", str(module), str(out), str(req)], check=True)
        result = json.loads(out.read_text())
        for field, fmt, addr in [("submode","B",entity+1), ("timer","H",entity+6),
                                 ("y","f",entity+0x18), ("work04","f",work+4),
                                 ("work10","f",work+0x10)]:
            assert struct.pack(">"+fmt, result[field]) == cpu.mem_read(addr, struct.calcsize(fmt)), (i,field)
print(f"{len(ys)*len(speeds)} Hot Shot settle cases matched installed PPC")
