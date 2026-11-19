"""Generate A3VE table-action parity cases; no cartridge assets are exported."""
import argparse,hashlib,json
from pathlib import Path
from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
from ball_reference import ROM_SHA256,STATE,SENTINEL
BASE=0x08000000
WIDTH={0x3046c:3,0x389c:1,0x37d4:2,0x5724:3}
def make(rom):
    assert hashlib.sha256(rom).hexdigest()==ROM_SHA256
    u=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
    for a,n in [(BASE,0x800000),(STATE,0x40000),(0x03000000,0x8000)]:u.mem_map(a,n)
    u.mem_write(BASE,rom)
    def put(a,n,v):u.mem_write(a,v.to_bytes(n,'little'))
    def callback(u,a,n,_):
        off=a-BASE
        if off in WIDTH:
            calls.append([off,[u.reg_read(r) for r in regs[:WIDTH[off]]]])
            for a,n,v in mutations.get(off,[]):put(a,n,v);writes.update(range(a,a+n))
            u.reg_write(UC_ARM_REG_PC,u.reg_read(UC_ARM_REG_LR))
    def write(u,access,a,n,v,_):
        if STATE<=a<0x03007000:writes.update(range(a,a+n))
    regs=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2]
    u.hook_add(UC_HOOK_CODE,callback);u.hook_add(UC_HOOK_MEM_WRITE,write)
    common=[[0x080b1a28+2*i,2,v] for i,v in enumerate([65533,255,32768,255,65535,32767])]
    cases=[]
    for off in [0x2dff8,0x2f9b4,0x3de48,0x2f634,0x468a8]:
        for k in range(32):
            table=k%5;count=k%4;alt=STATE+0x400
            args={0x2dff8:[],0x2f9b4:[0x89abcdef+k,65535+k,0xfedcba98],0x3de48:[],0x2f634:[0xabcd0000+k],0x468a8:[k%4,k+65535]}[off]
            init=[[0x030040b0,4,STATE],[0x03003068,2,table],[STATE+0x28,2,8],[STATE+0x76,2,65535],[STATE+0x6e,2,[0,5,9][k%3]],[0x020355e2,2,count],[0x02035746,2,k%2],
                  [0x02035138,2,4 if k%3==1 else 0],[0x0203513a,2,3 if k%4==0 else 0],[0x03002948,2,7 if k%2 else 0],[0x02032ae0,2,[29,30,31,65535][k%4]]]
            for a,n in [(0x020328c0,0xb4),(0x02032980,0x16),(0x020329d0,0x10),(0x020355b0,0x18),(0x020360f0,0x18),(0x02035fa0,0xe)]:
                init.extend([a+j,4,0x7f7f7f7f] for j in range(0,n-3,4))
            mutations={}
            if k%4==1:mutations={0x37d4:[[STATE+0x6e,2,6]],0x3046c:[[0x030040b0,4,alt],[alt+0x6e,2,0]]}
            if k%4==2:mutations={0x389c:[[0x030040b0,4,alt],[alt+0x6e,2,0]]}
            u.mem_write(STATE,bytes(0x40000));u.mem_write(0x03000000,bytes(0x8000));before={}
            for a,n,v in common+init:put(a,n,v);before.update({a+i:b for i,b in enumerate(v.to_bytes(n,'little'))})
            for r,v in zip(regs,args):u.reg_write(r,v)
            writes=set();calls=[];u.reg_write(UC_ARM_REG_SP,0x03007f00);u.reg_write(UC_ARM_REG_LR,SENTINEL|1)
            u.emu_start(BASE+off+1,SENTINEL,count=30000);assert u.reg_read(UC_ARM_REG_PC)==SENTINEL
            diff=[[a,u.mem_read(a,1)[0]] for a in sorted(writes) if before.get(a,0)!=u.mem_read(a,1)[0]]
            cases.append(dict(offset=off,args=args,init=init,mutations=mutations,calls=calls,diff=diff))
    spans=[('table_action_setup',0x2dff8,0x2e024),('action_setup_table1',0x2e024,0x2e0d8),('action_setup_table2',0x2e0d8,0x2e1a0),('action_setup_table3',0x5c4f8,0x5c544),('action_slot',0x468a8,0x468d0),('action_flag',0x2f634,0x2f640),('action_set_flag',0x2f614,0x2f634),('action_enqueue',0x2f9b4,0x2f9e0),('action_multiball',0x3de48,0x3df68),('action_table_flags',0x3d444,0x3d484),('action_timer_init',0x4302c,0x43048),('action_timer_start',0x43048,0x430ac),('action_palette_start',0x2a294,0x2a2c8)]
    return dict(schemaVersion=1,romSha256=ROM_SHA256,routines=[dict(name=n,offset=a,length=b-a,sha256=hashlib.sha256(rom[a:b]).hexdigest()) for n,a,b in spans],resources=[dict(name='multiball_positions',offset=0xb1a28,length=12,sha256=hashlib.sha256(rom[0xb1a28:0xb1a34]).hexdigest())],common=common,cases=cases)
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('rom',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    a.output.write_text(json.dumps(make(a.rom.read_bytes()),separators=(',',':'))+'\n')
