"""Execute A3VE collision routines with synthetic ball state and both synthetic and native table data.
Only the GBA BIOS math wrappers use controlled scalar returns; every collision routine runs from the original ROM.
The ID-0 retail cases retain ID-1 layers; ID-4 cases inherit ID-3 flipper setup. No boot or save state is implied.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import struct
from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
from unicorn.arm_const import UC_ARM_REG_LR, UC_ARM_REG_PC, UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3, UC_ARM_REG_SP
import spp_vectors  # noqa: E402

SHA = "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b"
BASE, STATE, POINT, STOP = 0x08000000, 0x02000000, 0x02001000, 0x03007f00



def flipper_cases(cpu,rom):
    rng=random.Random(0x2c3ac)
    masks=0x08800000
    cpu.mem_map(masks,0x100000)
    counts=[max(rom[0xaf914+i*70:0xaf914+(i+1)*70])+1 for i in range(3)]
    for bank,frames in enumerate([counts[0],max(counts[1:])]):
        data=bytearray(frames*96*96*2)
        for frame in range(frames):
            for y in range(96):
                for x in range(96):
                    filled=(10<=x<=80 and 6<=y<=78 and (x+3*y+frame+bank)%11!=0)
                    value=((x*61+y*283+frame*0x140+bank*0x500)&0xfff0)|int(filled)
                    struct.pack_into("<H",data,((frame*96+y)*96+x)*2,value)
        cpu.mem_write(masks+bank*0x80000,bytes(data))
    rows=[]
    dispatch=[]
    calls=[]
    def bios(uc,address,size,data):
        if address not in (0x0805fef4,0x0805feac):
            return
        x=uc.reg_read(UC_ARM_REG_R0)
        if address==0x0805fef4:
            value=math.isqrt(x)
            calls.append([8,x,0,value])
        else:
            x=struct.unpack("<h",struct.pack("<H",x&65535))[0]
            y=struct.unpack("<h",struct.pack("<H",uc.reg_read(UC_ARM_REG_R1)&65535))[0]
            value=round(math.atan2(y,x)*65536/(2*math.pi))&65535
            calls.append([10,x,y,value])
        uc.reg_write(UC_ARM_REG_R0,value)
        uc.reg_write(UC_ARM_REG_PC,uc.reg_read(UC_ARM_REG_LR))
    hook=cpu.hook_add(UC_HOOK_CODE,bios,begin=0x0805feac,end=0x0805fef4)
    for i in range(400):
        table=i%5
        state=bytearray(0x200)
        point=[rng.randint(12,60),rng.randint(12,60)]
        origins=[[1000,1000] for _ in range(4)]
        for j in range(4):
            if i%5==0 or j==i%4:
                origins[j]=[rng.randrange(-5,5),rng.randrange(-5,5)]
        origins[1][1]=origins[0][1]
        if i%4==1: origins[0][1]=origins[1][1]=0
        if i>=240:
            side=i%4
            table=2 if side==2 else (1 if i%8<4 else 2) if side==3 else i%5
            origins=[[1000,1000] for _ in range(4)]
            origins[side]=[0,0]
            if side<2:
                origins[0][1]=origins[1][1]=0
            point=[rng.randrange(25,60),rng.randrange(25,60)]
        phase=i%4
        state[2]=phase
        state[4]=9
        flag=int(i%6==0 and i<240)
        struct.pack_into("<H",state,0x28,flag)
        for side in range(4):
            struct.pack_into("<h",state,0x110+side*2,origins[side][0])
            if side!=1: struct.pack_into("<h",state,[0x10a,0x10a,0x10c,0x10e][side],origins[side][1])
        frame_inputs=[rng.randrange(13) for _ in range(4)]
        directions=[rng.choice([-1,0,1]) for _ in range(4)]
        if i>=240:
            directions[i%4]=1
        for j in range(4):
            state[0xc8+12*j]=frame_inputs[j]
            state[0xcb+12*j]=0xfe
            state[0xce+12*j]=directions[j]&255
        struct.pack_into("<II",state,0x1d8,masks,masks+0x80000)
        cpu.mem_write(STATE,bytes(state))
        cpu.mem_write(0x03003068,struct.pack("<H",table))
        cpu.mem_write(POINT,struct.pack("<2hH",*point,0xbabe))
        cpu.reg_write(UC_ARM_REG_R0,POINT)
        cpu.reg_write(UC_ARM_REG_R1,POINT+4)
        cpu.reg_write(UC_ARM_REG_SP,0x03007600)
        cpu.reg_write(UC_ARM_REG_LR,STOP+1)
        cpu.emu_start(BASE+0x2c3ad,STOP,count=100000)
        assert cpu.reg_read(UC_ARM_REG_PC)==STOP
        after=cpu.mem_read(STATE,len(state))
        assert all(a==b for j,(a,b) in enumerate(zip(after,state)) if j not in [4,0xcb,0xd7,0xe3,0xef])
        rows.append([table,point,origins,phase,flag,frame_inputs,directions,bool(cpu.reg_read(UC_ARM_REG_R0)),
                     after[4],struct.unpack("<H",cpu.mem_read(POINT+4,2))[0],[after[0xcb+12*j] for j in range(4)]])
        # Full dispatch using a verified zero region in the original cartridge
        # as an empty layer. Table selection installs slot 1, while slot 0
        # remains active. Flipper masks and ball state are synthetic.
        blank=0x0833cb70
        assert rom[blank-BASE:blank-BASE+6000]==bytes(6000)
        for ptr in (0x118,0x158,0x198):
            struct.pack_into("<II",state,ptr,blank,blank)
        struct.pack_into("<H",state,0x6c,1)
        for j in range(4):
            struct.pack_into("<2h",state,0xc4+12*j,100 if i>=240 or i%3==0 else 0,70 if i>=240 or i%3==0 else 0)
            state[0xca+12*j]=int(i%7==0)
        state[0x21]=int(i%6==0)
        struct.pack_into("<4h",state,0xa0,*point,*point)
        struct.pack_into("<3h",state,0xa8,rng.randint(-511,511),rng.randint(-511,511),rng.randint(-20,20))
        cpu.mem_write(STATE,bytes(state))
        cpu.mem_write(0x02032980,bytes(22))
        cpu.mem_write(0x02032770,struct.pack("<I",0x12345678))
        cpu.mem_write(0x030045c4,struct.pack("<I",255))
        cpu.mem_write(0x030040f0,bytes(1024))
        cpu.mem_write(0x02034f50,bytes(60))
        cpu.mem_write(0x03007600-80,struct.pack("<H",0xbabe))
        cpu.reg_write(UC_ARM_REG_SP,0x03007600)
        cpu.reg_write(UC_ARM_REG_LR,STOP+1)
        calls.clear()
        cpu.emu_start(BASE+0x2ba19,STOP,count=2000000)
        assert cpu.reg_read(UC_ARM_REG_PC)==STOP, i
        after=cpu.mem_read(STATE,len(state))
        changes=[[j,v] for j,v in enumerate(after[:0x108]) if v!=state[j]]
        initial=[[j,v] for j,v in enumerate(state[:0x108]) if v]
        banks=[[struct.unpack_from("<I",after,p+4*j)[0] for p in (0x118,0x158,0x198)] for j in range(2)]
        events=list(struct.unpack("<30H",cpu.mem_read(0x02034f50,60)))
        trace=list(struct.unpack("<256I",cpu.mem_read(0x030040f0,1024)))
        dispatch.append([initial,changes,banks,struct.unpack("<I",cpu.mem_read(0x030045c4,4))[0],
            struct.unpack("<I",cpu.mem_read(0x02032770,4))[0],[[j,v] for j,v in enumerate(events) if v],
            [[j,v] for j,v in enumerate(trace) if v],list(calls)])
    retail=[]
    for i in range(1000):
        table=i%5
        initializer=table or 1
        # ID 0 preserves the installed layer. Native 2b990 has valid slots
        # for IDs 1..4; exercise ID 0 with real layers previously set by ID 1.
        cpu.mem_write(STATE,bytes(0x200))
        cpu.mem_write(0x03003068,struct.pack("<H",initializer))
        def run(entry):
            cpu.reg_write(UC_ARM_REG_SP,0x03007600)
            cpu.reg_write(UC_ARM_REG_LR,STOP+1)
            cpu.emu_start(BASE+entry+1,STOP,count=2000000)
            assert cpu.reg_read(UC_ARM_REG_PC)==STOP,(i,hex(entry))
        run(0x2b990)
        # Table 4 leaves the flipper mask pointer unchanged. Install it via
        # original table-3 initialization before exercising that code path.
        if table==4:
            cpu.mem_write(0x03003068,struct.pack("<H",3))
            run(0x2a778)
        cpu.mem_write(0x03003068,struct.pack("<H",table))
        run(0x2a778)
        st=bytearray(cpu.mem_read(STATE,0x200))
        origins=[[struct.unpack_from("<h",st,0x110+j*2)[0],
                  struct.unpack_from("<h",st,[0x10a,0x10a,0x10c,0x10e][j])[0]] for j in range(4)]
        side=(i//5)%4
        if i%7<5:
            x,y=origins[side]
            point=[max(3,x*2+rng.randrange(4,91)),max(3,y*2+rng.randrange(4,91))]
        else:
            point=[rng.randrange(8,480),rng.randrange(8,[800,800,960,680,326][table])]
        # Bound the point to the known native layer extent. Inputs remain
        # synthetic ball state, while every tile, angle, material and mask
        # sampled by the runtime is read from unmodified cartridge memory.
        point[1]=min(point[1],[810,810,990,680,326][table])
        old=[x+rng.randint(-2,2) for x in point]
        struct.pack_into("<4h",st,0xa0,*point,*old)
        struct.pack_into("<3h",st,0xa8,rng.randint(-512,512),rng.randint(-512,512),rng.randint(-100,100))
        struct.pack_into("<2i",st,0x90,point[0]*128,point[1]*128)
        st[2]=i%4
        st[6]=(i//25)%2
        struct.pack_into("<2H",st,0x6c,st[6],i%3)
        st[0x21]=rng.choice([0,0,0,1,255])
        st[0x22]=rng.choice([0,0,0,1,255])
        for j in range(4):
            st[0xc8+12*j]=rng.randint(0,12)
            st[0xca+12*j]=rng.randint(0,1)
            st[0xce+12*j]=rng.choice([1,255])
            struct.pack_into("<2h",st,0xc4+12*j,rng.choice([0,70,140]),70)
        flags=[rng.randrange(3) for _ in range(11)]
        get_banks=lambda state:[[struct.unpack_from("<I",state,p+4*j)[0] for p in (0x118,0x158,0x198)] for j in range(2)]
        initial_banks=get_banks(st)
        cpu.mem_write(STATE,bytes(st))
        cpu.mem_write(0x02032980,struct.pack("<11H",*flags))
        cpu.mem_write(0x02032770,struct.pack("<I",0x12345678))
        cpu.mem_write(0x030045c4,struct.pack("<I",255))
        cpu.mem_write(0x030040f0,bytes(1024))
        cpu.mem_write(0x02034f50,bytes(60))
        cpu.mem_write(0x03007600-80,struct.pack("<H",0xbabe))
        calls.clear()
        run(0x2ba18)
        after=cpu.mem_read(STATE,0x200)
        changes=[[j,v] for j,v in enumerate(after[:0x108]) if v!=st[j]]
        initial=[[j,v] for j,v in enumerate(st[:0x108]) if v]
        events=list(struct.unpack("<30H",cpu.mem_read(0x02034f50,60)))
        trace=list(struct.unpack("<256I",cpu.mem_read(0x030040f0,1024)))
        retail.append([table,origins,flags,initial_banks,initial,changes,get_banks(after),
            struct.unpack("<I",cpu.mem_read(0x030045c4,4))[0],struct.unpack("<I",cpu.mem_read(0x02032770,4))[0],
            [[j,v] for j,v in enumerate(events) if v],[[j,v] for j,v in enumerate(trace) if v],list(calls)])
    cpu.hook_del(hook)
    return rows,dispatch,retail

def dynamic_cases(cpu,rom):
    rng=random.Random(0x2d70c)
    rows=[]
    bounds=[struct.unpack_from("<5H",rom,0xaebb0+10*i) for i in range(11)]
    for i in range(660):
        region=i%11
        layer,left,right,top,bottom=bounds[region]
        table=[1]*3+[2]*6+[3]*2
        table=table[region]
        if i%13==0:
            table=rng.choice([0,4])
        if i%7==0:
            layer=1-layer
        gate=i%2
        flags=[rng.randrange(4) for _ in range(11)]
        x=rng.choice([left-1,left,left+1,right,right+1])
        y=rng.choice([top-1,top,top+1,bottom,bottom+1])
        args=[int(x/8),int(y/8),x-int(x/8)*8,y-int(y/8)*8]
        state=bytearray(0x200)
        struct.pack_into("<2H",state,0x6c,layer,gate)
        cpu.mem_write(STATE,bytes(state))
        cpu.mem_write(0x03003068,struct.pack("<H",table))
        cpu.mem_write(0x02032980,struct.pack("<11H",*flags))
        cpu.mem_write(POINT,struct.pack("<4h",*args))
        for j,reg in enumerate([UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3]):
            cpu.reg_write(reg,POINT+2*j)
        cpu.reg_write(UC_ARM_REG_SP,0x03007600)
        cpu.reg_write(UC_ARM_REG_LR,STOP+1)
        cpu.emu_start(BASE+0x2d70d,STOP,count=10000)
        assert cpu.reg_read(UC_ARM_REG_PC)==STOP
        after=cpu.mem_read(STATE,len(state))
        pointers=[struct.unpack_from("<I",after,at+layer*4)[0] for at in [0x118,0x158,0x198]]
        args=struct.unpack("<4h",cpu.mem_read(POINT,8))
        rows.append([table,layer,gate,flags,[x,y],pointers if table in (1,2,3) else None,
                     [args[0]*8+args[2],args[1]*8+args[3]],cpu.reg_read(UC_ARM_REG_R0)])
    return rows
def reference(path):
    rom = Path(path).read_bytes()
    assert hashlib.sha256(rom).hexdigest() == SHA
    cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE)
    for address, size in [(STATE, 0x40000), (0x03000000, 0x8000), (BASE, len(rom))]:
        cpu.mem_map(address, size)
    cpu.mem_write(BASE, rom)
    cpu.mem_write(0x030040b0, struct.pack("<I", STATE))
    angles = bytes((i * 73 + 11) & 255 for i in range(64))
    cpu.mem_write(STATE + 0x4000, angles)
    rng = random.Random(0x2bfd4)
    rows = []
    responses = []
    for index in range(384):
        material = index % 256
        prior = [rng.randrange(2, 30) for _ in range(2)]
        delta = [rng.randrange(-2, 5) for _ in range(2)]
        offset = [0, 0] if index % 3 == 0 else [rng.randrange(-2, 3) for _ in range(2)]
        pos = [max(2, a+b) for a,b in zip(prior, delta)]
        seed = rng.randrange(65536)
        state = bytearray(0x200)
        struct.pack_into("<4h", state, 0xa0, *pos, *prior)
        struct.pack_into("<2b", state, 0x21, *offset)
        for at, ptr in [(0x118,0x2000), (0x158,0x4000), (0x198,0x5000)]:
            struct.pack_into("<I",state,at,STATE+ptr)
        for at in (0x10a,0x110,0x112):
            struct.pack_into("<h",state,at,1000)
        enable = index % 3 != 0
        state[0x2a:0x6a] = bytes([int(enable)])*64
        cpu.mem_write(STATE,bytes(state))
        table = 4 if index % 2 else 0
        cpu.mem_write(0x03003068,struct.pack("<H",table))
        phase = index % 5
        cpu.mem_write(STATE+0x5000, bytes(material if (i+phase)%5==0 else 0 for i in range(64)))
        sp = 0x03007600
        cpu.mem_write(sp-52,struct.pack("<H",seed))
        cpu.reg_write(UC_ARM_REG_R0,POINT)
        cpu.reg_write(UC_ARM_REG_LR,STOP+1)
        cpu.reg_write(UC_ARM_REG_SP,sp)
        cpu.emu_start(BASE+0x2bfd5,STOP,count=100000)
        assert cpu.reg_read(UC_ARM_REG_PC)==STOP
        after = cpu.mem_read(STATE,len(state))
        assert all(a==b for i,(a,b) in enumerate(zip(after,state)) if i not in (4,5,0x23,0x24))
        rows.append([table,material,phase,enable,prior,pos,offset,seed,
                     list(struct.unpack("<2h",cpu.mem_read(POINT,4))),
                     cpu.reg_read(UC_ARM_REG_R0), list(after[4:6]), list(after[0x23:0x25])])
        velocity = [rng.randint(-32768,32767) for _ in range(3)]
        fixed = rng.choice([0x77ff,0x7800,-16,0x123456])
        latch = rng.choice([0,1,2,255])
        counter = rng.choice([0,255,0xffffffff])
        struct.pack_into("<3h",state,0xa8,*velocity)
        struct.pack_into("<i",state,0x90,fixed)
        state[0x26]=latch
        cpu.mem_write(STATE,bytes(state))
        cpu.mem_write(0x02032770,struct.pack("<I",0x12345678))
        cpu.mem_write(0x030045c4,struct.pack("<I",counter))
        cpu.mem_write(0x030040f0,bytes(1024))
        cpu.mem_write(0x02034f50,bytes(56))
        cpu.mem_write(sp-80,struct.pack("<H",seed))
        cpu.reg_write(UC_ARM_REG_LR,STOP+1)
        cpu.reg_write(UC_ARM_REG_SP,sp)
        cpu.emu_start(BASE+0x2ba19,STOP,count=100000)
        assert cpu.reg_read(UC_ARM_REG_PC)==STOP
        after=cpu.mem_read(STATE,len(state))
        changes=[[i,v] for i,v in enumerate(after) if v!=state[i]]
        assert all(i<0xc4 for i,_ in changes)
        new_counter,=struct.unpack("<I",cpu.mem_read(0x030045c4,4))
        angle,=struct.unpack("<I",cpu.mem_read(0x02032770,4))
        history=list(struct.unpack("<256I",cpu.mem_read(0x030040f0,1024)))
        events=list(struct.unpack("<28H",cpu.mem_read(0x02034f50,56)))
        responses.append([velocity,fixed,latch,counter,changes,new_counter,angle,
                          [[i,v] for i,v in enumerate(history) if v],
                          [[i,v] for i,v in enumerate(events) if v]])
    def span(name, offset, length):
        return {"name":name,"offset":offset,"length":length,
                "sha256":hashlib.sha256(rom[offset:offset+length]).hexdigest()}
    routines=[span(name,a,b-a) for name,a,b in [
        ("collision_dispatch",0x2ba18,0x2bfd4),
        ("probe",0x2bfd4,0x2c140),("sweep",0x2c140,0x2c238),
        ("classify_tiles",0x2c238,0x2c3ac),("classify_flippers",0x2c3ac,0x2c56c),
        ("sample_flipper",0x2c56c,0x2cab4),("contact_adjust",0x2cab4,0x2cbd4),
        ("reflect",0x2cbd4,0x2cd20),("material_response",0x2cd20,0x2cf7c),
        ("select_map",0x2d70c,0x2df8c),("extra_flipper_kick",0x2cf7c,0x2d3f0),("main_flipper_kick",0x2d3f0,0x2d698)]]
    resources=[span(name,a,n) for name,a,n in [
        ("flipper_frame_selectors",0xaf914,210),("flipper_neighbours",0x6e93a0,8),
        ("collision_regions",0xaebb0,110),("base_tile_pointers",0x6e9340,24),
        ("base_angle_pointers",0x6e9360,24),("base_material_pointers",0x6e9380,24),
        ("flipper_separations",0xaf844,16),("kick_lookup_window",0xaf844,208),("flipper_mask_planes",0x66a0b8,21*96*96*2),("wall_separations",0xaf824,32),("material_impulses",0xafa2c,18)]]
    flippers,dispatch,retail=flipper_cases(cpu,rom)
    return {"schemaVersion":1,"romSha256":SHA,"routines":routines,"resources":resources,
            "dynamic":dynamic_cases(cpu,rom),"flippers":flippers,"flipperDispatch":dispatch,"retail":retail,
            "responses":responses,"cases":rows}

if __name__ == "__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--rom",required=True)
    p.add_argument("--generate",action="store_true")
    args=p.parse_args()
    result=reference(args.rom)
    out=spp_vectors.path("collision-vectors.json")
    if args.generate:
        out.write_text(json.dumps(result,separators=(",",":"))+"\n")
    else:
        assert json.loads(out.read_text())==result
    total=sum(len(result[key]) for key in ("cases","responses","dynamic","flippers","flipperDispatch","retail"))
    print(f"Verified {total} original collision routine calls, including {len(result['retail'])} on native maps and masks.")
