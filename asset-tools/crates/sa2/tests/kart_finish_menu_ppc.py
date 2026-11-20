"""Compare restricted finish input and timeouts with the installed PPC task."""
import json, struct, subprocess, sys, tempfile
from gsne8p import steam_rel
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_BIG_ENDIAN, UC_MODE_PPC32, UC_HOOK_CODE
from unicorn import ppc_const as R

cli, rel, tmp = map(Path, sys.argv[1:])
data = rel.read_bytes()
u = Uc(UC_ARCH_PPC, UC_MODE_BIG_ENDIAN | UC_MODE_PPC32)
code, ram = 0x81000000, 0x82000000
u.mem_map(code,0x10000)
u.mem_write(code,data[232:232+0x10000])
u.mem_map(ram,0x10000)
task, work = ram+0x100, ram+0x200
pointers = {(0,0x801e5398):ram+0x300,(0,0x803ad380):ram+0x308,
            (80,0x3ce24):ram+0x30c,(80,0xd734):code+0xd734,(80,0xd954):code+0xd954}
callsites = {}
off,n = struct.unpack_from(">II",data,40)
for mod,at in struct.iter_unpack(">II",data[off:off+n]):
    while True:
        delta,kind,sec,add = steam_rel(struct.unpack_from(">HBBI", data, at), mod);at += 8
        if kind == 203: break
        if kind == 202: target,loc = sec,0;continue
        loc += delta
        if target != 1 or not 0xd540 <= loc < 0xd734: continue
        if kind == 10: callsites[code+loc] = add
        elif kind in (4,6):
            ptr = pointers[mod,add]
            u.mem_write(code+loc,struct.pack(">H",(ptr if kind==4 else (ptr+32768)>>16)&65535))
def intercept(machine,addr,*unused):
    target = callsites[addr]
    arg = machine.reg_read(R.UC_PPC_REG_3)
    if target == 0x80071af0:
        assert arg == task
        calls.append(["engine_80071888_task",None])
    else:
        if target == 0x8006b978:
            assert all(machine.reg_read(getattr(R,f"UC_PPC_REG_{n}"))==0 for n in (4,5,6))
        calls.append([f"engine_{target:x}",arg])
    machine.reg_write(R.UC_PPC_REG_PC,addr+4)
for addr in callsites: u.hook_add(UC_HOOK_CODE,intercept,begin=addr,end=addr)
def put(addr,fmt,*v): u.mem_write(addr,struct.pack(">"+fmt,*v))
def get(addr,fmt): return struct.unpack(">"+fmt,u.mem_read(addr,struct.calcsize(">"+fmt)))[0]
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for i,(mode,sel,timer,b0,b1) in enumerate([
        (0,0,299,0,0),(0,0,300,0,0),(0,1,65535,0,0),
        (1,1,42,0x80,0x41),(1,0,42,0,0x40),(1,0,9,0,0x81),(1,2,9,0x80,0),
        (1,1,9,1,0),(1,0,9,0,1),(1,0,3599,0,0),(1,1,3600,0,0),
        (1,0,65535,0,0),(2,0,42,0,1),(-1,0,42,0,1)]):
        state=dict(mode=mode,selection=sel,timer=timer,flag_3ce24=7)
        req=Path(td)/"in.json";out=Path(td)/f"{i}.json"
        req.write_text(json.dumps(dict(state=state,input_words=[b0,b1],confirm_mask=1)))
        subprocess.run([cli,"kart-finish-menu",rel,out,req],check=True)
        want=json.loads(out.read_text());calls=[]
        put(task+0x34,"I",work);put(work,"b",mode);put(work+3,"B",sel);put(work+6,"H",timer)
        put(ram+0x300,"4I",b0,b1,1,7)
        u.reg_write(R.UC_PPC_REG_3,task);u.reg_write(R.UC_PPC_REG_1,ram+0xe000)
        u.reg_write(R.UC_PPC_REG_LR,code+0x200)
        u.emu_start(code+0xd540,code+0x200,count=200)
        assert u.reg_read(R.UC_PPC_REG_PC)==code+0x200
        got=dict(mode=get(work,"b"),selection=get(work+3,"B"),timer=get(work+6,"H"),flag_3ce24=get(ram+0x30c,"I"))
        assert (got,calls)==(want["state"],want["calls"]), i
print("Matched 14 restricted finish task updates and all imported-call arguments.")
