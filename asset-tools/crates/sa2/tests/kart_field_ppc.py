"""Compare kart field writes and call dispatch with installed CartD PowerPC."""
import json, struct, subprocess, sys, tempfile
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_BIG_ENDIAN, UC_MODE_PPC32, UC_HOOK_CODE
from unicorn import ppc_const as R

cli, rel, tmp = map(Path, sys.argv[1:])
data = rel.read_bytes()
code, ram = 0x81000000, 0x82000000
u = Uc(UC_ARCH_PPC, UC_MODE_BIG_ENDIAN | UC_MODE_PPC32)
u.mem_map(code, 0x10000)
u.mem_write(code, data[232:232+0x10000])
u.mem_map(ram, 0x10000)
def reg(n,v): u.reg_write(getattr(R,"UC_PPC_REG_"+str(n)),v)
def put(addr,v): u.mem_write(addr,struct.pack(">I",v & 0xffffffff))
def get(addr): return struct.unpack(">i",u.mem_read(addr,4))[0]
def run(a,b):
    u.emu_start(code+a,code+b,count=3000)
    assert u.reg_read(R.UC_PPC_REG_PC) == code+b
def intercept(machine,addr,*args):
    offset = addr-code
    # Slot being processed is identified by the current sorted-index pointer.
    slot = get(machine.reg_read(R.UC_PPC_REG_23))
    arg = machine.reg_read(R.UC_PPC_REG_4) if offset==0x5c38 else None
    calls.append([slot,f"cart_{offset:x}",arg])
    machine.reg_write(R.UC_PPC_REG_PC,machine.reg_read(R.UC_PPC_REG_LR))
for target in (0x5c38,0x7040,0xd4f8):
    u.hook_add(UC_HOOK_CODE,intercept,begin=code+target,end=code+target)
fields = {"work_44":0x44,"place":0xc0,"first_ticks":0x298,"sixth_ticks":0x29c,"cooldown":0x2a0}
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for k,(counter,restricted,suppress,latched,clock,cooldown,change) in enumerate([
        (2,False,False,False,701,301,False),(2,False,False,False,700,301,False),
        (2,False,False,False,701,300,False),(2,False,False,False,701,301,True),
        (3,True,False,False,701,301,False),(3,False,False,False,701,301,False),
        (2,False,True,False,701,301,False),(3,True,False,True,701,301,False)]):
        slots=[]; calls=[]
        put(ram+0x300,int(latched));put(ram+0x304,23);put(ram+0x308,int(suppress))
        put(ram+0x400+0x7c,clock);u.mem_write(ram+0x500,bytes([restricted]))
        for i in range(6):
            task=ram+0x1000+0x500*i;work=task+0x100;put(task+0x38,work)
            put(ram+0x600+4*i,task);put(ram+0x700+4*i,i)
            row=dict(work_44=int(i!=1),place=(3 if change else 6-i),first_ticks=14,sixth_ticks=14,cooldown=cooldown)
            for name,off in fields.items(): put(work+off,row[name])
            put(work+0xc4,counter)
            slots.append(dict(**row,standing=dict(counter=counter,segment=i,segment_length=10,distance_to_next=1)))
        field=dict(finish_latched=latched,post_finish_ticks=23,suppress_place_calls=suppress,
                   restrict_finish=restricted,race_clock=clock,slots=slots)
        req=Path(td)/"req.json";out=Path(td)/f"{k}.json";req.write_text(json.dumps(field))
        subprocess.run([cli,"kart-field-update",rel,out,req],check=True)
        result=json.loads(out.read_text())
        reg(3,ram+0x300);run(0x8d78,0x8d84 if latched else 0x8d9c)
        if latched:
            reg(4,ram+0x304);run(0x8d8c,0x913c)
        else:
            reg(20,0);reg(3,ram+0x304);run(0x8da8,0x8dac)
            for n,v in {20:0,21:ram+0x600,22:ram+0x300,23:ram+0x700,28:ram+0x308,
                        29:6,30:ram+0x400,31:ram+0x500}.items():reg(n,v)
            run(0x8f58,0x913c)
        assert calls == result["calls"]
        assert bool(get(ram+0x300)) == result["finish_latched"]
        assert get(ram+0x304) == result["post_finish_ticks"]
        for i,row in enumerate(result["slots"]):
            for name,off in fields.items():assert row[name]==get(ram+0x1100+0x500*i+off)
print("Matched eight six-kart field updates, finish dispatch order and timer writes.")
