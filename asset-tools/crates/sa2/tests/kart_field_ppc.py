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
# Relocate the three native cue-table references and run the local selector.
sec5, size5 = struct.unpack_from(">II",data,struct.unpack_from(">I",data,16)[0]+40)
bank=0x83000000
u.mem_map(bank,(size5+4095)&~4095)
u.mem_write(bank,data[sec5 & ~1:(sec5 & ~1)+size5])
off,n=struct.unpack_from(">II",data,40)
for mod,at in struct.iter_unpack(">II",data[off:off+n]):
    while True:
        delta,kind,sec,add=struct.unpack_from(">HBBI",data,at);at+=8
        if kind==203:break
        if kind==202:target,loc=sec,0;continue
        loc+=delta
        if mod==80 and target==1 and sec==5 and (0x5c38<=loc<0x5cc0 or 0x2a2c<=loc<0x2b14) and kind in (4,6):
            ptr=bank+add
            u.mem_write(code+loc,struct.pack(">H",(ptr if kind==4 else (ptr+32768)>>16)&65535))
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
    if offset==0x5c38 and observed_index is not None:
        cues.append(dict(slot=slot,kind=arg,work_48=observed_index,resolution=dict(state="silent")))
        return
    if offset==0x5c38:
        cues.append(dict(slot=slot,kind=arg,work_48=None,resolution=dict(state="needs_kart_index")))
    machine.reg_write(R.UC_PPC_REG_PC,machine.reg_read(R.UC_PPC_REG_LR))
def external_cue(machine,addr,*args):
    cues[-1]["resolution"]=dict(state="engine_request",id=machine.reg_read(R.UC_PPC_REG_3))
    machine.reg_write(R.UC_PPC_REG_PC,addr+4)
u.hook_add(UC_HOOK_CODE,external_cue,begin=code+0x5cc8,end=code+0x5cc8)
reg(1,ram+0xe000)
for target in (0x5c38,0x7040,0xd4f8):
    u.hook_add(UC_HOOK_CODE,intercept,begin=code+target,end=code+target)
fields = {"work_44":0x44,"place":0xc0,"first_ticks":0x298,"sixth_ticks":0x29c,"cooldown":0x2a0}
with tempfile.TemporaryDirectory(dir=tmp) as td:
    for k,(counter,restricted,suppress,latched,clock,cooldown,change) in enumerate([
        (2,False,False,False,701,301,False),(2,False,False,False,700,301,False),
        (2,False,False,False,701,300,False),(2,False,False,False,701,301,True),
        (3,True,False,False,701,301,False),(3,False,False,False,701,301,False),
        (2,False,True,False,701,301,False),(3,True,False,True,701,301,False)] + [(count,False,False,False,701,301,False) for index in list(range(16))+list(range(2,8)) for count in (2,3)]):
        slots=[]; calls=[]; cues=[]
        observed_index=None if k<8 else (k-8)//2
        if k>=40:
            observed_index=2+(k-40)//2
            reg(3,observed_index);u.reg_write(R.UC_PPC_REG_LR,code+0x200)
            run(0x2a2c,0x200)
        put(ram+0x300,int(latched));put(ram+0x304,23);put(ram+0x308,int(suppress))
        put(ram+0x400+0x7c,clock);u.mem_write(ram+0x500,bytes([restricted]))
        for i in range(6):
            task=ram+0x1000+0x500*i;work=task+0x100;put(task+0x38,work)
            put(ram+0x600+4*i,task);put(ram+0x700+4*i,i)
            row=dict(work_44=int(i!=1),place=(3 if change else 6-i),first_ticks=14,sixth_ticks=14,cooldown=cooldown)
            for name,off in fields.items(): put(work+off,row[name])
            if observed_index is not None:
                row["work_48"]=observed_index
                put(work+0x48,observed_index)
            put(work+0xc4,counter)
            slots.append(dict(**row,standing=dict(counter=counter,segment=i,segment_length=10,distance_to_next=1)))
        field=dict(finish_latched=latched,post_finish_ticks=23,suppress_place_calls=suppress,
                   restrict_finish=restricted,race_clock=clock,slots=slots)
        if k>=40:field["alternate_profiles"]=list(range(2,observed_index+1))
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
        assert cues == result["cue_requests"], (k,cues,result["cue_requests"])
        assert bool(get(ram+0x300)) == result["finish_latched"]
        assert get(ram+0x304) == result["post_finish_ticks"]
        for i,row in enumerate(result["slots"]):
            for name,off in fields.items():assert row[name]==get(ram+0x1100+0x500*i+off)
print("Matched 52 field updates, all 16 original cue rows and six native replacements, three selectors, and missing-index reports.")
