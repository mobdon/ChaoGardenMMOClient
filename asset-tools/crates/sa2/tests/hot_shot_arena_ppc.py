"""Compare Rust with the installed Hot Shot stage update; run from the checkout root."""
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

module = Path(os.environ["SA2_GC_REL_DIR"]) / "boss_hotshotD.rel"
raw = module.read_bytes()
assert hashlib.sha256(raw).hexdigest() == "0989db26abb7559760ff7b49e6156c249b36b97211a18b59fc343e63ddcf460d"
word = lambda off: struct.unpack_from(">I", raw, off)[0]
section = lambda n: word(word(16) + n * 8) & ~3
cpu = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
code, data, player, stack, stop = 0x81000000, 0x82000000, 0x200000, 0x300800, 0x81007080
slot = 0x801e7768
for addr, size in [(code + 0x6000, 8192), (data, 4096), (player, 4096),
                   (slot & ~4095, 4096), (stack & ~4095, 4096)]:
    cpu.mem_map(addr, size)
lo, hi = 0x6fcc, 0x7070
cpu.mem_write(code + lo, raw[section(1)+lo:section(1)+hi])
cpu.mem_write(data, raw[section(4):section(4)+4096])
cpu.mem_write(slot, struct.pack(">I", player))
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
        assert (owner, sec) in [(83, 4), (0, 7)] and kind in (4, 6)
        address = data + add if owner == 83 else add
        half = (address + 0x8000) >> 16 if kind == 6 else address
        cpu.mem_write(code + off, struct.pack(">H", half & 65535))

cpu.reg_write(regs.UC_PPC_REG_MSR, cpu.reg_read(regs.UC_PPC_REG_MSR) | 8192)
def neighbors(value):
    bits = struct.unpack(">I", struct.pack(">f", value))[0]
    return [struct.unpack(">f", struct.pack(">I", bits+i))[0] for i in (-1, 0, 1)]

xs = neighbors(-200) + [-0.0] + neighbors(200)
zs = neighbors(-175) + [0.0] + neighbors(175)
local = Path.cwd() / ".local"
local.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=local) as tmp:
    for i, (x, z) in enumerate(itertools.product(xs, zs)):
        y = -0.0 if i % 2 else 123.25
        cpu.mem_write(player+0x14, struct.pack(">fff", x, y, z))
        cpu.reg_write(regs.UC_PPC_REG_1, stack)
        cpu.reg_write(regs.UC_PPC_REG_LR, stop)
        cpu.emu_start(code+0x7050, stop, count=100)
        assert cpu.reg_read(regs.UC_PPC_REG_PC) == stop
        out = Path(tmp) / f"{i}.json"
        subprocess.run([sys.argv[1], "hot-shot-arena", str(module), str(out),
                        str(x), str(y), str(z)], check=True)
        actual = json.loads(out.read_text())["position"]
        assert struct.pack(">fff", *actual) == cpu.mem_read(player+0x14, 12), (x, y, z, actual)
print("49 Hot Shot arena updates matched installed PPC bit for bit")
