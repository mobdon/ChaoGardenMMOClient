"""Compare alternate path task work and engine call order at init and teardown."""
import json, os, tempfile, subprocess
from pathlib import Path
import wild_canyon_ppc as n
u,R=n.u,n.R
sites=(0x220e4,0x22108,0x222c4,0x222d4)
def watch(cpu,pc,*_):
    site=pc-n.base[1]
    if site==sites[0]:
        calls.append([site,n.reg(3),n.reg(4)]);cpu.reg_write(R.UC_PPC_REG_3,allocation)
    elif site==sites[1]:
        assert n.reg(3)==n.child
        calls.append([site]);cpu.reg_write(R.UC_PPC_REG_3,saved)
    elif site==sites[2]:
        assert n.reg(3)==n.child
        calls.append([site,n.reg(4)])
    else:calls.append([site,n.reg(3)])
    cpu.reg_write(R.UC_PPC_REG_PC,pc+4)
for site in sites:u.hook_add(n.UC_HOOK_CODE,watch,begin=n.base[1]+site,end=n.base[1]+site)
cut=n.base[1]+0x22114
u.hook_add(n.UC_HOOK_CODE,lambda cpu,*_:cpu.emu_stop(),begin=cut,end=cut)
# Resolve the register-loaded address from the relocated instructions, not a fixed BSS assumption.
hi=n.get(n.base[1]+0x220ca,"H")[0];lo=n.get(n.base[1]+0x220ce,"h")[0];guard=(hi<<16)+lo
def run(off):
    u.reg_write(R.UC_PPC_REG_1,n.stage+4000);u.reg_write(R.UC_PPC_REG_3,n.child);u.reg_write(R.UC_PPC_REG_LR,n.stop)
    u.emu_start(n.base[1]+off,n.stop,count=100);return u.reg_read(R.UC_PPC_REG_PC)
cases=[]
for flag in (-1,0,1):
 for has_set in (False,True):
  for allocation in (0,0x100900):
   for saved in (0,0x1234,0xffffffff):
    calls=[];n.put(guard,"i",flag);n.put(n.child+0x30,"II",int(has_set),n.work)
    n.put(n.child+0x38,"fII",0.625,0x1357,0x100800)
    pc=run(0x220ac);assert pc in (n.stop,cut)
    before=list(n.get(n.child+0x38,"fII"));assert run(0x22298)==n.stop
    after=list(n.get(n.child+0x38,"fII"))
    cases.append([flag,has_set,allocation,saved,pc==cut,before,after,calls])
with tempfile.TemporaryDirectory(dir=".local") as tmp:
    p=Path(tmp)/"work.json";p.write_text(json.dumps(cases))
    subprocess.run(["cargo","test","--locked","--manifest-path","asset-tools/crates/sa2/Cargo.toml",
                    "--test","emerald_lifetime","--","--ignored"],check=True,
                   env=os.environ|{"SA2_PATH_WORK_ORACLE":str(p.resolve())})
print(len(cases),"initialization/teardown sequences matched")
