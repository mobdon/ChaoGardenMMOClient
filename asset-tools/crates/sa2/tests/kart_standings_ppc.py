"""Compare score arithmetic and native tie ordering; never start the game."""
import json, math, struct, subprocess, sys, tempfile
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_BIG_ENDIAN, UC_MODE_PPC32
from unicorn import ppc_const as R

cli, rel, tmp = map(Path, sys.argv[1:])
data = rel.read_bytes()
u = Uc(UC_ARCH_PPC, UC_MODE_BIG_ENDIAN | UC_MODE_PPC32)
code, ram = 0x81000000, 0x82000000
u.mem_map(code, 0x10000)
u.mem_write(code, data[232:232+0x10000])
u.mem_map(ram, 0x10000)
def reg(n, val):
    u.reg_write(getattr(R, "UC_PPC_REG_"+str(n)), val)
def fp(n, val):
    reg("FPR"+str(n), struct.unpack(">Q", struct.pack(">d", val))[0])
def put(at, fmt, *v):
    u.mem_write(at, struct.pack(">"+fmt, *v))
def run(a, b):
    u.emu_start(code+a, code+b, count=4096)
    assert u.reg_read(R.UC_PPC_REG_PC) == code+b
reg("MSR", 8192)
sp = ram+0xf000
put(ram, "f", 0)
put(ram+4, "f", 100)
put(ram+8, "d", 4503601774854144.)
with tempfile.TemporaryDirectory(dir=tmp) as td:
    req = Path(td)/"req.json"
    for index, case in enumerate([[], [(0,0,1,0)], [(v,0,8,2) for v in [4,4,1,5,2,4]],
                 [(1,-1,5,d) for d in [2,6,5,0,1,4]],
                 [(1,0,0,0),(3,0,0,1),(2,0,4,2)],
                 [(2147483647,2147483647,7,1),(-3,-11,9,2)]]):
        req.write_text(json.dumps([dict(zip(("segment","counter","segment_length","distance_to_next"),row)) for row in case]))
        out = Path(td)/f"out-{index}.json"
        subprocess.run([cli,"kart-standings",rel,out,req],check=True)
        doc = json.loads(out.read_text())
        for i,(seg,ctr,length,dist) in enumerate(case):
            reg(1,sp);reg(19,0x43300000);reg(22,ram+0x1000);reg(23,ram+0x2000+4*i)
            reg(26,ram);reg(27,ram+4);reg(28,1000000)
            put(ram+0x10bc,"i",seg);put(ram+0x10c4,"i",ctr)
            fp(1,dist);fp(31,length)
            run(0x8ea0,0x8ed8)
            reg(3,ram+8);run(0x8ee0,0x8f00)
            bits, = struct.unpack(">I",u.mem_read(ram+0x2000+4*i,4))
            expected = doc["score_bits"][i]
            def nan(v): return math.isnan(struct.unpack(">f",struct.pack(">I",v))[0])
            assert bits == expected or (nan(bits) and nan(expected)), (bits,expected)
        reg(1,sp);reg(3,ram+0x2000);reg(4,len(case));reg(5,ram+0x3000)
        run(0x94f4,0x951c)
        reg(3,ram+0x4000)  # imported temporary allocator result
        run(0x9520,0x96e0)
        for i in range(len(case)):
            slot, = struct.unpack(">I",u.mem_read(ram+0x3000+4*i,4))
            reg(20,i);reg(29,len(case));reg(19,ram+0x1000)
            run(0x8f70,0x8f78)
            place, = struct.unpack(">I",u.mem_read(ram+0x10c0,4))
            assert doc["places_by_slot"][slot] == place
print("Matched native score arithmetic and slot places across six fields.")
