"""Local PowerPC oracle: execute the installed key task and compare mounted Rust frames."""
import hashlib,json,os,struct,subprocess,sys,tempfile
from pathlib import Path
from unicorn import Uc, UC_ARCH_PPC, UC_MODE_PPC32, UC_MODE_BIG_ENDIAN, UC_HOOK_CODE
from unicorn import ppc_const as R
game=Path(os.environ["SA2_GAME_PATH"]); gd=game/"resource/gd_PC"
B,D,T,W,G=0x81000000,0x82000000,0x100000,0x100100,0x801cbd88
FIRST,END=0x25114,0x252d0
def words(d,addr):return struct.unpack_from('>I',d,addr)[0]
def image(level):
    d=(gd/f"stg{level:02}D.rel").read_bytes()
    sec=lambda i:words(d,words(d,16)+8*i)&~1
    refs={}
    for mod,at in struct.iter_unpack('>II',d[words(d,40):words(d,40)+words(d,44)]):
        off=0
        while True:
            inc,kind,s,v=struct.unpack_from('>HBBI',d,at);at+=8
            if kind==203:break
            if kind==202:section=s;off=0;continue
            off+=inc;refs[section,off]=(mod,s,v,kind)
    rows= [k for (t,k),(m,s,v,kind) in refs.items() if t==5 and kind==1 and d[sec(s)+v:sec(s)+v+9].lower()==b'contchao\0']
    init,={refs[5,row-4][2] for row in rows if (5,row-4) in refs and refs[5,row-4][1]==1}
    def target(pc):
        ins=words(d,sec(1)+pc);assert ins&0xfc000003==0x48000001
        a=ins&0x3fffffc;return pc+(a if a<0x2000000 else a-0x4000000)
    spawn=target(init+0x748);cb=refs[1,spawn+0x36][2]
    code=bytearray(d[sec(1)+cb:sec(1)+cb+END-FIRST])
    for off in (0xd0,0x13c,0x15c):
        dest=target(cb+off)
        if off==0xd0:assert d[sec(1)+dest:sec(1)+dest+8]==bytes.fromhex('fc200a104e800020')
        if off==0x13c:assert words(d,sec(1)+dest)==0x4e800020
        struct.pack_into('>I',code,off,0x48000001)
    assert hashlib.sha256(code).hexdigest()=="ecb24e66ce5db8bc559b93918eac137898b51d562f0978a1e1d9dd2d14fe96ec"
    for off,val in zip((0x46,0xb2,0xd6,0xf2,0x106,0x142,0x192),(1145569280,3206125978,1050253722,1065101558,1034147594,1075838976,512)):
        m,s,a,k=refs[1,cb+off];assert m==words(d,0) and words(d,sec(s)+a)==val
    for off,addr in [(0x50,2147718768),(0x78,2147948664),(0x12c,2147510004),(0x170,2147897672),(0x17c,2147948664),(0x188,2147546348)]:
        m,_,a,k=refs[1,cb+off];assert (m,k,a)==(0,10,addr)
    return d,sec,refs
for level in (3,4,5,6,7,8,9,10,11,12,13,14,16,17,18,21,22,23,24,25,26,27,28,30,32,40,43,44):image(level)
d,sec,refs=image(13)
assert hashlib.sha256(d).hexdigest()=="bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b"
code=bytearray(d[sec(1)+FIRST:sec(1)+END])
u=Uc(UC_ARCH_PPC,UC_MODE_PPC32|UC_MODE_BIG_ENDIAN)
seen=set()
def page(p):
    a=p&~4095
    if a not in seen:u.mem_map(a,4096);seen.add(a)
def put(p,fmt,v):u.mem_write(p,struct.pack('>'+fmt,v))
def get(p,fmt):return struct.unpack('>'+fmt,u.mem_read(p,struct.calcsize('>'+fmt)))[0]
def reg(i,v):u.reg_write(getattr(R,'UC_PPC_REG_'+str(i)),v)
for (t,off),(mod,s,a,k) in refs.items():
    if t!=1 or not FIRST<=off<END:continue
    val=(D+s*0x400000+a) if mod else a
    page(val)
    if mod:u.mem_write(val,d[sec(s)+a:sec(s)+a+4])
    if k in (4,6):struct.pack_into('>H',code,off-FIRST,(val if k==4 else (val+32768)>>16)&65535)
    else:
        assert k==10;struct.pack_into('>I',code,off-FIRST,0x48000001|((val-(B+off))&0x3fffffc))
for p in [B+FIRST,B+0x79f8,B+0x24fc0,B+0x24eac,T,T+0x2000,B+END]:page(p)
u.mem_write(B+FIRST,bytes(code));u.mem_write(B+0x79f8,d[sec(1)+0x79f8:sec(1)+0x7a00])
u.reg_write(R.UC_PPC_REG_MSR,u.reg_read(R.UC_PPC_REG_MSR)|8192)
actions={0x800066f4:'checkHitPlayer',0x80071878:'freeTask',refs[1,0x25164][2]:'queryWithin800',0x8000f4ec:'registerCollision',B+0x24eac:'startPickupTask'}
def hook(uc,pc,size,_):
    name=actions[pc];calls.append(name)
    if name=='queryWithin800':reg(3,int(f['within_800']))
    if name=='checkHitPlayer':reg(3,T+800 if f.get('player_contact') else 0)
for pc in actions:
    page(pc);put(pc,'I',0x4e800020);u.hook_add(UC_HOOK_CODE,hook,begin=pc,end=pc)
put(B+0x24fc0,'I',0x4e800020)
def bits(x):return struct.unpack('>I',struct.pack('>f',x))[0]
with tempfile.TemporaryDirectory(dir=Path.cwd()/'.local') as tmp:
    tmp=Path(tmp);count=0
    for case in range(3):
        pos=[620.,-1237.,3047.];floor=pos[1];keyframes=[];native=[]
        u.mem_write(T,bytes(0x400));put(T+0x34,'I',W);put(W+1,'B',1)
        for i,val in enumerate(pos):put(W+20+4*i,'f',val)
        put(W+32,'f',floor);put(W+36,'f',2.5)
        for tick in range(3610):
            f={};calls=[]
            if case==0 and tick==240:f['player_contact']=True
            if case==1 and tick==10:f.update(game_byte_44=1,within_800=True)
            if case==2 and tick>=3601:f['within_800']=tick<3606
            put(G+0x44,'b',f.get('game_byte_44',0));reg(1,T+0x2ff0);reg(3,T);u.reg_write(R.UC_PPC_REG_LR,B+END)
            u.emu_start(B+FIRST,B+END,count=180);assert u.reg_read(R.UC_PPC_REG_PC)==B+END
            keyframes.append(f);native.append((get(T+0x4c,'i'),get(W+12,'i'),get(W+1,'B'),get(W+24,'I'),get(W+36,'I'),calls))
            if 'freeTask' in calls:break
        else:raise AssertionError('expected task cleanup')
        request={'stage_flags':0,'frames':[{}, {'impact':{'rolls':[[0]*3]*14}}, {}, {}, {}, {}, {'release':{'has_condition':True,'flag_20':False,'broken':False,'has_key':False}}],'key':{'allocation_ok':True,'frames':keyframes}}
        req=tmp/'input.json';req.write_text(json.dumps(request));out=tmp/f'output{case}.json'
        subprocess.run([sys.argv[1],'--source-root',str(game),'fan-replay',str(game/'sonic2app.exe'),str(out),os.environ['SA2_PC_CONFIG'],'Hero Story Sequence','13','0','136',str(req)],check=True)
        frames=json.loads(out.read_text())['replay']['key']['frames']
        for tick,(v,n) in enumerate(zip(frames,native,strict=True)):
            calls=[next(iter(c)) if isinstance(c,dict) else c for c in v['calls']]
            assert (v['age'],v['yaw'],v['mode'],bits(v['position'][1]),bits(v['speed']),calls)==n,(case,tick,v,n)
        count+=len(frames)
print(count,'mounted key frames matched installed PPC')
