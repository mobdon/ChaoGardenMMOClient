"""Execute installed stage registration prologs and compare the descriptor given to the engine."""
import json, os, struct, subprocess, sys, tempfile
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"])
def expected(d):
    w=lambda i:struct.unpack_from(">I",d,i)[0]
    mod=w(0);section={};base={};u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
    for s in (1,5):
        row=w(16)+8*s; at=w(row)&~1;size=w(row+4);base[s]=0x81000000+s*0x200000
        u.mem_map(base[s],(size+4095)&~4095);u.mem_write(base[s],d[at:at+size])
    prolog=w(52);target=None
    for imported,at in struct.iter_unpack(">II",d[w(40):w(40)+w(44)]):
        off=0
        while True:
            inc,k,s,a=struct.unpack_from(">HBBI",d,at);at+=8
            if k==203:break
            if k==202:dst=s;off=0;continue
            off+=inc
            if imported==0 and dst==1 and off==prolog+20 and k==10:target=a
            if imported!=mod or dst not in base or s not in base:continue
            val=base[s]+a
            if k==1:u.mem_write(base[dst]+off,struct.pack(">I",val))
            if k in (4,6):u.mem_write(base[dst]+off,struct.pack(">H",(val if k==4 else (val+32768)>>16)&65535))
    assert target==0x8001a01c
    result={}
    def register(cpu,pc,size,_):
        ptr=cpu.reg_read(R.UC_PPC_REG_3);raw=bytes(cpu.mem_read(ptr,24))
        callbacks=[v-base[1] if v else None for v in struct.unpack_from(">III",raw,12)]
        result.update(module=mod,prolog=prolog,registerTarget=target,descriptor=ptr-base[5],name=raw[:12].split(b"\0")[0].decode(),
                      startup=callbacks[0],teardown=callbacks[1],entry=callbacks[2])
        cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
    site=base[1]+prolog+20;u.hook_add(UC_HOOK_CODE,register,begin=site,end=site)
    u.mem_map(0x100000,4096);u.reg_write(R.UC_PPC_REG_1,0x100f00);u.reg_write(R.UC_PPC_REG_LR,0x100100)
    u.emu_start(base[1]+prolog,0x100100,count=30);assert u.reg_read(R.UC_PPC_REG_PC)==0x100100
    return result
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    count=0
    for path in sorted((game/"resource/gd_PC").glob("stg*D.rel")):
        out=Path(tmp)/(path.stem+".json")
        ref=expected(path.read_bytes())
        proc=subprocess.run([sys.argv[1],"--source-root",str(game),"stage-registration",str(path),str(out)],capture_output=True,text=True)
        if proc.returncode:
            raise AssertionError((path.name,proc.stderr))
        assert json.loads(out.read_text())==ref,path.name
        count+=1
    assert count==58
    print(count,"installed numbered stage registrations matched")
