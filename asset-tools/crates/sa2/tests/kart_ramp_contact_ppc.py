"""Compare active jump-panel task branches with the installed PPC instructions."""
import hashlib,json,struct,subprocess,sys,tempfile
from gsne8p import steam_rel
from pathlib import Path
from unicorn import Uc,UC_ARCH_PPC,UC_MODE_PPC32,UC_MODE_BIG_ENDIAN,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn import ppc_const as R

cli,rel,private=map(Path,sys.argv[1:4])
dash=sys.argv[4:]==["dash"]
d=rel.read_bytes()
assert hashlib.sha256(d).hexdigest()=="71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c"
word=lambda i:struct.unpack_from(">I",d,i)[0]
base={1:0x81000000,4:0x82000000,5:0x83000000,6:0x84000000}
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
for sec,addr in base.items():
    row=word(16)+8*sec;off=word(row)&~1;size=word(row+4)
    u.mem_map(addr,(size+4095)&~4095)
    if off:u.mem_write(addr,d[off:off+size])
for mod,at in struct.iter_unpack(">II",d[word(40):word(40)+word(44)]):
    while True:
        delta,k,s,a=steam_rel(struct.unpack_from(">HBBI", d, at), mod);at+=8
        if k==203:break
        if k==202:target=s;loc=0;continue
        loc+=delta
        if dash and target==5 and mod==80 and k==1 and s in base:
            u.mem_write(base[5]+loc,struct.pack(">I",base[s]+a))
        if target==1 and mod==80 and (0xbdf4<=loc<0xbf30 or 0xac40<=loc<0xaf34 or 0x1714<=loc<0x17e8 or 0x5b90<=loc<0x5c30 or 0x979c<=loc<0x9810) and k in (4,6):
            ptr=base[s]+a
            if loc==0x1716:registry=ptr
            if loc==0x97aa:course=ptr
            if loc==0x97c2:indices=ptr
            u.mem_write(base[1]+loc,struct.pack(">H",(ptr if k==4 else (ptr+32768)>>16)&65535))
ram=0x85000000;u.mem_map(ram,0x10000);work=ram;coll=ram+256
get=lambda n:u.reg_read(getattr(R,f"UC_PPC_REG_{n}"))
def reg(n,v):u.reg_write(getattr(R,f"UC_PPC_REG_{n}"),v)
def put(p,fmt,*v):u.mem_write(p,struct.pack(">"+fmt,*v))
def read(p,fmt):return struct.unpack(">"+fmt,u.mem_read(p,struct.calcsize(">"+fmt)))
u.reg_write(R.UC_PPC_REG_MSR,8192)
kart=ram+0x2000;kart2=ram+0x2200
def hook(u,addr,size,_):
    off=addr-base[1]
    if dash:
        if off==0xac60:
            reg(3,int(culled))
        elif off==0xacb0:
            assert get(3)==panel
            installs.extend([v-base[1] for v in read(panel+20,"2I")]);init.extend([get(4)-base[5],get(5),get(6)])
        elif off==0xae0c:registered.append(get(3))
        elif off==0xaf20:
            uv.extend([get(5),*read(get(6),"2h")])
            return # Run the native material directory loop, including halfword writes.
        else:
            off={0xad68:0xbe90,0xad70:0xbe98,0xad7c:0xbea4,0xad8c:0xbeb4,0xad90:0xbeb8,
                 0xad9c:0xbec4,0xadc0:0xbee8,0xaddc:0xbf04}.get(off,off)
            return handle_contact(u,addr,off)
        u.reg_write(R.UC_PPC_REG_PC,addr+4)
        return
    handle_contact(u,addr,off)
def handle_contact(u,addr,off):
    if off not in (0xbe90,0xbe98,0xbea4,0xbeb4,0xbeb8,0xbec4,0xbee8,0xbf04,0x97ec,0x97fc,0x5c0c):return
    i=get(26)
    if off==0xbea4:trace.append(dict(contact_index=i,task=get(25),yaw=struct.unpack(">i",struct.pack(">I",get(4)))[0],call_8002f8c4=None,called_5b90=False,call_8006c97c=None,call_8006b710=None))
    if off==0xbeb4:trace[-1]["matrix_input"]=list(read(get(4),"3f"))
    if off==0xbec4:
        assert get(3)==trace[-1]["task"]
    if off==0xbee8:trace[-1]["call_8002f8c4"]=[get(n) for n in (3,4,5,6)]
    if off==0xbf04:
        assert get(3)==get(25) and get(4)==0
        trace[-1]["called_5b90"]=True
        return # execute both native helpers
    if off in (0x97ec,0x97fc):
        trace[-1]["call_8006c97c"]=struct.unpack(">i",struct.pack(">I",get(3)))[0]
        changed=obs[i]["work_48_after_call"]
        if changed is not None:put(get(25)+128+72,"i",changed)
    if off==0x5c0c:trace[-1]["call_8006b710"]=[get(n) for n in (3,4,5,6)]
    u.reg_write(R.UC_PPC_REG_PC,addr+4)
u.hook_add(UC_HOOK_CODE,hook)
def uv_write(m,access,address,size,value,_):
    pc=m.reg_read(R.UC_PPC_REG_PC)-base[1]
    if pc in (0xdf9c,0xdfb0):
        assert size==2
        value=struct.unpack(">h",struct.pack(">H",value&65535))[0]
        if pc==0xdf9c:writes.append(dict(section5_offset=address-base[5],uv=[value]))
        else:
            assert address==base[5]+writes[-1]["section5_offset"]+2
            writes[-1]["uv"].append(value)
if dash:u.hook_add(UC_HOOK_MEM_WRITE,uv_write,begin=base[5],end=base[5]+0x2fffff)
with tempfile.TemporaryDirectory(dir=private) as tmp:
    for case,(flags,timer,ids,results) in enumerate([
        (0,1,[kart],[1]),(1,0,[],[]),(1,0,[0,ram+0x2600],[0,0]),
        (1,0.25,[kart,kart2,kart2],[0,-1,0]),(3,3.5,[kart2],[1]),(1,1,[kart],[1])]+[(1,0,[kart2],[slot]) for slot in range(9)]):
        trace=[];obs=[]
        put(registry,"6I",kart,0,0,0,0,kart2);reg(31,work);reg(1,ram+0x8000)
        put(work+12,"i",-1234);put(work+32,"3f",11.5,-2.25,timer);put(work+44,"I",coll)
        put(coll+2,"hH",len(ids),flags)
        for i,task in enumerate(ids):
            put(coll+20+i*8,"I",task)
            enabled=int(task==kart2)
            if task:put(task+56,"I",task+128);put(task+128+68,"I",enabled);put(task+128+112,"I",37)
            slot=results[i] if case>=6 else 2
            byte=[49,50,71,72,255][case%5]
            changed=7 if case==6 else None
            if task:
                put(task+128+72,"i",slot);put(task+128+188,"I",1)
            put(course+8,"I",ram+0x9100);put(indices+4,"I",3);put(ram+0x9103,"B",byte)
            obs.append(dict(task=task,post_response_work_44=enabled,post_response_work_70=37,post_response_work_48=slot,course_byte=byte,work_48_after_call=changed))
        if dash:
            phase=[0,16383,65535,16384][case%4];mode=0 if case==0 else 1;culled=case==1
            panel=ram+0x700;put(panel+0x34,"I",work);put(work,"B",mode);put(work+6,"H",phase)
            installs=[];init=[];registered=[];uv=[];writes=[];reg(3,panel)
            u.emu_start(base[1]+0xac54,base[1]+0xae28,count=3000)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xae28
        else:
            u.emu_start(base[1]+0xbdf4,base[1]+0xbf30,count=3000)
            assert u.reg_read(R.UC_PPC_REG_PC)==base[1]+0xbf30
        req=Path(tmp)/"request.json";out=Path(tmp)/f"{case}.json"
        q=dict(cooldown=timer,collision_flags=flags,registered_tasks=[kart,0,0,0,0,kart2],yaw=-1234,work_20=11.5,work_24=-2.25,contacts=obs)
        expected=dict(cooldown=read(work+40,"f")[0],dispatches=trace)
        if dash:
            del q["work_24"];q.update(strength=q.pop("work_20"),culled=culled,mode=mode,phase=phase)
            expected.update(mode=read(work,"b")[0],phase=read(work+6,"H")[0],install_callbacks=installs or None,
                            collision_init=init or None,register_collision=registered==[panel],uv_call=uv or None,uv_writes=writes)
            assert len(writes)==(0 if culled else 32)
        if dash and trace:
            q["responses"]=[];expected["vehicle_writes"]=[]
            for hit in trace:
                # Matrix return is injected at the same boundary as the dedicated response oracle.
                response=dict(transformed_vector=[3,0,-4],work_274=-12,angle_return=-.625,magnitude_return=5,turn_return=-192)
                q["responses"].append(response);ref=dict(response)
                ref["vector"]=ref.pop("transformed_vector");ref["work_44"]=obs[hit["contact_index"]]["post_response_work_44"]
                req.write_text(json.dumps(ref));dest=Path(tmp)/f"response-{case}-{hit['contact_index']}.json"
                subprocess.run([cli,"kart-ramp-response",rel,dest,req],check=True)
                expected["vehicle_writes"].append(json.loads(dest.read_text()))
        req.write_text(json.dumps(q))
        subprocess.run([cli,"kart-dash-panel" if dash else "kart-ramp-tick",rel,out,req],check=True)
        assert json.loads(out.read_text())==expected,(case,expected)
print("Matched 15 native", "dash panel updates" if dash else "jump panel contact loops")
