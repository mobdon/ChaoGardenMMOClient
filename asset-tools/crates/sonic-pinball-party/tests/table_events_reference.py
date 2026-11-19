"""A3VE spatial-event oracle. Synthetic regions exercise every original handler without exporting retail asset tables."""
import argparse,hashlib,json,random,struct
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={0x468a8:2,0x3046c:3,0x2f9b4:3,0x2dff8:0,0x3de48:0}
GROUPS=[(0x086e93a8,6),(0x086e93d8,10),(0x086e9428,6),(0x086e9458,4),(0x086e9478,4),(0x086e9498,4)]
def make(rom):
    assert hashlib.sha256(rom).hexdigest()==ROM_SHA256
    u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x800000),(STATE,0x40000),(0x03000000,0x8000)]:u.mem_map(a,n)
    u.mem_write(BASE,rom)
    def put(a,n,v):u.mem_write(a,(v%2**(8*n)).to_bytes(n,'little'))
    def callback(u,a,n,_):
        off=a-BASE
        if off in WIDTH:
            calls.append([off,[u.reg_read(r) for r in [UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2][:WIDTH[off]]]])
            for a,n,v in mutations.get(off,[]):put(a,n,v);writes.update(range(a,a+n))
            u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
    def write(u,access,a,n,v,_):
        if STATE<=a<0x03007000:writes.update(range(a,a+n))
    u.hook_add(UC_HOOK_CODE,callback);u.hook_add(UC_HOOK_MEM_WRITE,write)
    common=[];coords=[]
    for g,(base,count) in enumerate(GROUPS):
        for i in range(count):
            x=10+13*i;y=100+g*20
            if g==1 and i==2:x=-10
            for j,v in enumerate([x,x+4,y,y+4]):common.append([base+8*i+2*j,2,v&65535])
            for k in range(8):coords.append(([0,0,2,2,3,3][g],x+[0,4,2,-1,5,0,1,2][k],y+[0,4,1,0,0,2,1,2][k],g,i,k))
    for a,n in [(0x080afb87,1),(0x080b00d0,1),(0x086e94b8,2)]:
        for j in range(200):common.append([a+n*j,n,(j+1)%2])
    for a,n,v in common:put(a,n,v)
    for x,y in [(5,233),(41,301),(42,301),(227,209),(227,208),(66,76),(74,88),(46,96),(59,106),(226,200)]:
        for k in range(8):coords.append((2,x,y,9,0,k))
    cases=[]
    def run(table,x,y,g,i,k):
        alt=STATE+0x400;vx=[1,65535,0,1,65535,1,0,1][k];vy=[1,65535,0,1,65535,1,65535,1][k]
        init=[[0x030040b0,4,STATE],[0x03003068,2,table],[STATE+0x98,2,x&65535],[STATE+0x9a,2,y&65535],[STATE+0x28,2,int(k==7)],
          [STATE+0x10,2,[0,4,6,12,65535,32768,8,1][k]],[STATE+0xa8,2,vx],[STATE+0xaa,2,vy],
          [STATE+0xa0,2,65517],[STATE+0xa2,2,302],[STATE+0x8c,2,k%2],[STATE+6,1,[1,1,0,255,1,1,1,1][k]],
          [STATE+3,1,int(k==2)],[STATE+0x14,1,7],[STATE+0x6e,2,[0,1,10,0,2,0,0,0][k]],
          [0x020355e0,4,1+((max(i-7,0) if g==1 else 0)<<16)],[0x020329dc,2,int(k==4)],[0x020329d0,2,k%2],
          [0x02035fa0,2,int(k==2)],[0x02032968,2,int(k==3)],[0x020360f2,2,[0,1,2,0,1,0,0,0][k]]]
        mutations={}
        if k==5:mutations={o:[[0x030040b0,4,alt],[alt+6,1,128]] for o in WIDTH}
        u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000));before={}
        for a,n,v in init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
        return init,mutations,before
    def execute(row):
        nonlocal mutations,writes,calls
        init,mutations,before=run(*row);writes=set();calls=[]
        u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
        u.emu_start(BASE+0x2ea05,SENTINEL,count=10000)
        assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
        diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
        return dict(init=init,mutations=mutations,calls=calls,diff=diff)
    mutations={};writes=set();calls=[]
    for row in coords+[(4,0,0,0,0,0),(1,10,100,0,0,1)]:cases.append(execute(row))
    u.mem_write(BASE,rom)
    retail=hashlib.sha256()
    for t,x,y,g,i,k in coords[:272]:
        a,count=GROUPS[g];left,right,top,bottom=struct.unpack_from('<4h',rom,a-BASE+8*i)
        xx=[left,right,(left+right)//2,left-1,right+1,left,left+1,(left+right)//2][k]
        yy=[top,bottom,(top+bottom)//2,top,top,top+2,top+1,(top+bottom)//2][k]
        result=execute((t,xx,yy,g,i,k))
        retail.update(json.dumps([result['calls'],result['diff']],separators=(',',':')).encode()+b'\n')
    spans=[('event_dispatch',0x2ea04,0x2ea40),('events_table01',0x2ea40,0x2ef64),('events_table2',0x2ef64,0x2f388),('events_table3',0x2f388,0x2f614)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name=n,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for n,a,b in spans],resources=[dict(name=n,offset=a,length=l,sha256=hashlib.sha256(rom[a:a+l]).hexdigest()) for n,a,l in [('event_rectangles',0x6e93a8,0x110),('event_table3_layers',0x6e94b8,37*29*2),('event_table2_left_layers',0xafb87,41*33),('event_table2_right_layers',0xb00d0,41*33)]],common=common,cases=cases,retailDigest=retail.hexdigest())
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
