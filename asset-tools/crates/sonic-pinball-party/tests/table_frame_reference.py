"""Check the source table wrappers and message scheduler against local A3VE code."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={0x1dc4:1,0x2580:0,0x60894:2,0xa438:0,0x5668:0,0x2700:0,0xa668:1,0xa808:1,0xa9f0:1,0xaa20:1,0xa85c:1,
       0x2f92c:0,0x5784:3,0x2f14:2,0x2f68:0,0x2ff40:2,0x304a0:1,0x162c:3}
def make(rom):
    assert hashlib.sha256(rom).hexdigest()==ROM_SHA256
    u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x800000),(STATE,0x40000),(0x03000000,0x8000)]:u.mem_map(a,n)
    u.mem_write(BASE,rom)
    def put(a,n,v):u.mem_write(a,v.to_bytes(n,'little'))
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2]
    def cb(u,a,n,_):
        off=a-BASE
        if off in WIDTH and not (integrated and off in (0x2ff40,0x304a0)):
            calls.append([off,[u.reg_read(r) for r in regs[:WIDTH[off]]]])
            for a,n,v in mutations.get(off,[]):put(a,n,v);writes.update(range(a,a+n))
            u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
    def write(u,access,a,n,v,_):
        if STATE<=a<0x03007000:writes.update(range(a,a+n))
    u.hook_add(UC_HOOK_CODE,cb);u.hook_add(UC_HOOK_MEM_WRITE,write)
    cases=[];integrated=False
    for entry,count in [(0xab78,12),(0xabb4,40),(0x2fd68,120)]:
        for k in range(count):
            ctx=0x020199a0;aux=[0,1,10][k%3];depth=k%10+1
            kind=(k//8)%4;idx=[0,31,65535][k%3];nextidx=(idx+1)&31
            queue=k%8 in [3,4,5,7];mode=2+k%9
            score=[0,1,1234,98765432101234,2**63-1,2**63,2**64-1,1000000][k%8]
            init=[[0x03000e00,2,depth],[0x03000e2c+2*(depth-1),2,k%8],[0x02004db0,4,ctx],[ctx,4,k%2],[0x03002060,2,aux],[0x03002064+4*max(0,aux-1),4,0x0800ed81],
                  [0x0300296c,4,int(k%4!=0)],[0x03002964,2,k%3],[0x020319c0,4,[30,31,32,0xffffffff][k%4]],[0x020319a4,4,k%2],
                  [0x030040b0,4,STATE],[STATE+8,4,score&0xffffffff],[STATE+12,4,score>>32],[STATE+0x86,2,k%2],[STATE+0x88,2,[0,1,3,65535][k%4]],
                  [0x02035ff0,2,[0,1,255,3][k%4]],[0x02033f34,2,k%3],[0x02032aec,2,int(k%8!=6)],[0x02033f2c,2,kind],[0x02032ae4,2,mode],
                  [0x02033f28,2,idx],[0x02032ae0,2,nextidx if queue else idx],[0x02034f44,2,k%3],[0x0300077a,2,0x1344],[0x0300078a,2,0xffff],
                  [0x03000772,2,65535],[0x030007c2,2,65535],[0x03000782,2,99],[0x020329e0+8*nextidx,4,0x080afa84],
                  [0x020329e0+8*nextidx+4,2,8],[0x020329e0+8*nextidx+6,2,17]]
            if entry==0x2fd68 and k>=112:
                init.extend([[0x02033f2c,2,0],[0x02032aec,2,1],[0x02032ae0,2,idx]])
            mutations={}
            if k%4==1:mutations={0x2580:[[0x03002060,2,0]],0x2ff40:[[0x02033f2c,2,2]],0x304a0:[[STATE+0x86,2,1]]}
            if k%4==3:mutations={0x2700:[[ctx,4,1]],0x5784:[[0x020319c0,4,31]],0x162c:[[0x0300077a,2,0x1000]],0x2ff40:[[0x02033f2c,2,1]]}
            writes=set();calls=[];before={}
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            for a,n,v in init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            u.reg_write(UC_ARM_REG_R0,ctx);u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+entry+1,SENTINEL,count=300000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
            cases.append(dict(entry=entry,context=ctx,init=init,mutations=mutations,calls=calls,diff=diff))
    sequences=[];integrated=True
    for entry in [0x2ff40,0x2fd68]:
        for mode in (0,1,3,4,8,9,11):
            desc=STATE+0x1000
            init=[[0x030040b0,4,STATE],[0x02033f28,2,31],[0x02032ae0,2,2],[0x02033f34,2,9],[0x02032aec,2,1],[0x02034f44,2,55],[0x02033f2c,2,0],
                  [0x020329e0,4,desc],[0x020329e4,2,mode],[0x020329e6,2,2],[0x020329e8,4,desc],[0x020329ec,2,mode],[0x020329ee,2,3],
                  [desc+16,4,desc+128],[desc+132,2,4],[desc+24,2,9]]
            calls=[];writes=set();before={};mutations={}
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000))
            for a,n,v in init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            steps=1 if entry==0x2ff40 else 5
            for _ in range(steps):
                u.reg_write(UC_ARM_REG_R0,desc);u.reg_write(UC_ARM_REG_R1,mode);u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
                u.emu_start(BASE+entry+1,SENTINEL,count=300000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
            sequences.append(dict(entry=entry,mode=mode,steps=steps,desc=desc,init=init,calls=calls,diff=diff))
    spans=[('table_frame_update',0xab78,0xabb4),('table_frame_control',0xabb4,0xad44),('message_scheduler',0x2fd68,0x2ff40),('message_dequeue',0x2f9e0,0x2fa28),('score_text',0x30b78,0x30c58),('score_icon',0x30a0c,0x30b78)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name=n,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for n,a,b in spans],cases=cases,sequences=sequences)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
