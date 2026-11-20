"""Run CartD vehicle movement callbacks 8 and 9 against the Rust command."""
import json, struct, subprocess, sys, tempfile
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R

cli, rel, temp = map(Path, sys.argv[1:])
data = rel.read_bytes()
u = Uc(UC_ARCH_PPC, UC_MODE_PPC32 | UC_MODE_BIG_ENDIAN)
base = {1: 0x81000000, 4: 0x82000000, 5: 0x86000000, 6: 0x83000000}
for section, addr in base.items():
    row = struct.unpack_from(">I", data, 16)[0] + section * 8
    off, size = struct.unpack_from(">II", data, row)
    u.mem_map(addr, (size + 4095) & ~4095)
    if off:
        start = off & ~1
        u.mem_write(addr, data[start:start+size])
off, n = struct.unpack_from(">II", data, 40)
for mod, at in struct.iter_unpack(">II", data[off:off+n]):
    while True:
        delta, k, s, a = struct.unpack_from(">HBBI", data, at)
        at += 8
        if k == 203: break
        if k == 202:
            target, loc = s, 0
            continue
        loc += delta
        if target == 1 and (0x422c <= loc < 0x42c8 or 0x190c <= loc < 0x1968 or 0x5774 <= loc < 0x5b90) and k in (4, 6):
            assert mod == 80
            p = base[s] + a
            half = p if k == 4 else (p + 32768) >> 16
            u.mem_write(base[1]+loc, struct.pack(">H", half & 65535))
task = 0x84000000
u.mem_map(task, 4096)
tw, work = task+0x100, task+0x200
def put(addr, fmt, *v): u.mem_write(addr, struct.pack(">"+fmt,*v))
def read(addr, fmt): return struct.unpack(">"+fmt,u.mem_read(addr,struct.calcsize(">"+fmt)))
put(task+0x34,"II",tw,work)
u.reg_write(R.UC_PPC_REG_MSR,8192)
def observe(m,addr,*_):
    if addr == base[1]+0x5820:
        dispatch.update(path="normal_state5820")
        m.emu_stop()
        return
    cue,handle,arg5,arg6,pos=[m.reg_read(getattr(R,f"UC_PPC_REG_{k}")) for k in range(3,8)]
    assert pos == tw+20
    dispatch.update(path="engine8006b188",cue=cue,task_relative_handle=handle-task,
                    arg5=arg5,arg6=arg6-2**32,position=list(read(pos,"3f")))
    m.reg_write(R.UC_PPC_REG_PC,addr+4)
for pc in (0x5820,0x5b70):
    u.hook_add(UC_HOOK_CODE,observe,begin=base[1]+pc,end=base[1]+pc)
with tempfile.TemporaryDirectory(dir=temp) as td:
    for i,(state,count) in enumerate([(8,121),(9,-3),(9,119),(9,120),(9,121),(9,2147483647)]+[(8,0)]*16):
        q=dict(vehicle_state=state, position=[-4.5,8.25,99.125],angles=[5,-8,12],
               work_68=-91,work_dc=312,global_21760=count,global_22494=45)
        fields=dict(vehicle_state=(work+0x30,"i"),position=(tw+20,"3f"),angles=(tw+8,"3i"),
                    work_68=(work+0x68,"i"),work_dc=(work+0xdc,"i"),
                    global_21760=(base[6]+0x21760,"i"),global_22494=(base[6]+0x22494,"i"))
        for k,(addr,fmt) in fields.items():
            put(addr,fmt,*(q[k] if isinstance(q[k],list) else [q[k]]))
        u.reg_write(R.UC_PPC_REG_3,task)
        end=base[1]+0x200
        u.reg_write(R.UC_PPC_REG_LR,end)
        u.emu_start(base[1]+(0x422c if state==8 else 0x426c),end,count=64)
        assert u.reg_read(R.UC_PPC_REG_PC)==end
        enabled=int(i not in (1,5))
        profile=[14,3,9,15,10,11][i] if i<6 else i-6
        q["audio"]=dict(work_44=enabled,profile=profile)
        put(work+0x44,"i",enabled)
        put(work+0x48,"I",profile)
        u.reg_write(R.UC_PPC_REG_1,task+0xff0)
        u.reg_write(R.UC_PPC_REG_3,task)
        u.reg_write(R.UC_PPC_REG_LR,end)
        dispatch={}
        u.emu_start(base[1]+0x5774,end,count=160)
        if not enabled: dispatch.update(path="disabled")
        assert u.reg_read(R.UC_PPC_REG_PC)==(base[1]+0x5820 if dispatch.get("path")=="normal_state5820" else end)
        req,out=Path(td)/"input.json",Path(td)/f"{i}.json"
        req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-scripted-motion",rel,out,req],check=True)
        got=json.loads(out.read_text())
        assert got["audio"]==dispatch,(i,got,dispatch)
        for k,(addr,fmt) in fields.items():
            v=read(addr,fmt)
            assert got[k]==(list(v) if len(v)>1 else v[0]),(i,k,v,got[k])
print("Matched movement and audio dispatch in 22 cases, including all 16 cue profiles.")
