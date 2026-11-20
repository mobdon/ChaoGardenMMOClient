"""Compare Rust with the installed Hot Shot target rotation; run from the checkout root."""
import hashlib
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
code, data, table, work, stack = 0x81000000, 0x82000000, 0x83000000, 0x200000, 0x300000
lo, hi = 0x4d41c, 0x4d634
for addr in (code+0x4d000,data+0x1000,table+0x9a000,work,stack):
    cpu.mem_map(addr,4096)
cpu.mem_write(code+lo,raw[section(1)+lo:section(1)+hi])
cpu.mem_write(data+0x1b60,raw[section(4)+0x1b60:section(4)+0x1b78])
cpu.mem_write(table+0x9a9f0,raw[section(5)+0x9a9f0:section(5)+0x9aa08])
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
        if owner==0:
            assert kind==10 and add==0x8010dc88
            continue
        assert owner==83 and sec in (4,5) and kind in (4,6)
        address=(data if sec==4 else table)+add
        half = (address + 0x8000) >> 16 if kind == 6 else address
        cpu.mem_write(code + off, struct.pack(">H", half & 65535))

cpu.reg_write(regs.UC_PPC_REG_MSR, cpu.reg_read(regs.UC_PPC_REG_MSR) | 8192)
local=Path.cwd()/".local"
local.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(dir=local) as tmp:
    for i, varied in enumerate(([100,0,-50],[2147483647,-2147483648,2147483647],[-16777217,16777217,3])):
        fixed=[2147483647,0,-2147483648]
        samples=[0,16384,32767]
        cpu.mem_write(work+0x18,struct.pack(">6i",*fixed,*varied))
        cpu.reg_write(regs.UC_PPC_REG_27, work)
        cpu.reg_write(regs.UC_PPC_REG_1, stack+0x800)
        start=lo
        for call, sample in zip((0x4d458,0x4d504,0x4d59c),samples):
            cpu.emu_start(code+start,code+call,count=100)
            assert cpu.reg_read(regs.UC_PPC_REG_PC)==code+call
            cpu.reg_write(regs.UC_PPC_REG_3,sample)
            start=call+4
        cpu.emu_start(code+start,code+hi,count=100)
        assert cpu.reg_read(regs.UC_PPC_REG_PC)==code+hi
        req,out=Path(tmp)/f"{i}-request.json",Path(tmp)/f"{i}.json"
        req.write_text(json.dumps(dict(fixed=fixed,varied=varied,engine_samples=samples)))
        run=subprocess.run([sys.argv[1],"hot-shot-target-rotation",str(module),str(out),str(req)],capture_output=True)
        if i==1:
            assert run.returncode and b"fctiwz range" in run.stderr
            continue
        run.check_returncode()
        actual=json.loads(out.read_text())
        assert struct.pack(">6i",*actual["fixed"],*actual["varied"])==cpu.mem_read(work+0x18,24), (i,actual,struct.unpack(">6i",cpu.mem_read(work+0x18,24)))
print("Two rotation cases matched all six PPC words; out-of-range conversion rejected")

