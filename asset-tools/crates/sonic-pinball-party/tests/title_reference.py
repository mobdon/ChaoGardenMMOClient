"""Native A3VE title control/resources oracle with explicit external boundaries.

Original title wrappers/auxiliaries,8260/8358 interpolation and2614 submissions
execute. Listed frame/transform/palette/audio helpers are intercepted.39A0 writes
synthetic same-length resource data;162C copies actual supplied RAM. Default
cases use synthetic scroll words/descriptor-priority halfwords. Separate local
sequence uses authentic tables; authentic decoded asset hashes are independent.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import struct
import unittest

import control_reference as control
import interpolation_reference as interpolation
import reference_parity
import spp_vectors  # noqa: E402

FIXTURE = spp_vectors.path("title-vectors.json")
ROUTINES = {"constructor": (0x8260,0x8358), "interpolation": (0x8358,0x8414),
            "paletteFade": (0x8414,0x8450), "specialFade": (0x8450,0x8494),
            "timeoutFade": (0x8494,0x8520), "idle": (0x8520,0x88AC),
            "localizedResources": (0x88AC,0x8A04), "scroll": (0x8A04,0x8AAC),
            "wait": (0x8AAC,0x8AE0), "finish": (0x8AE0,0x8B20),
            "initialize": (0x8B20,0x90CC), "resume": (0x90CC,0x90D8),
            "restart": (0x90D8,0x9100), "update": (0x9100,0x91C4), "entry": (0x91C4,0x9264),
            "queueReset": (0x2580,0x258C)}
ARITY = {0x30FC:0,0x5618:1,0x5784:3,0x5724:3,0x5668:0,0x1DC4:1,0x3250:0,
         0x5F0BC:0,0x37D4:2,0x5EBD4:2,0x0884:0,0x5F040:0,0x3278:0,0x3010:0,
         0x2580:0,0x2700:0,0x236C:5,0x389C:1}
ASSETS = [0x082D7808,0x082DC684,0x082D87E0,0x082DC974,0x082D9924,0x082DCF1C,
          0x082E18D4,0x082E0ADC,0x082DCC60,0x082E0530,0x082DC058,0x082DACD4,
          0x082DB160,0x082DB414,0x082DB6CC,0x082DBAA0,0x082DBD88]
RESOURCE_SPANS = [(0x0806A34C,0x48),(0x086DD878,4),(0x086E7128,2),(0x086E7130,2),
                  (0x082E0C54,0x200),(0x082E0E54,0x200),(0x082E19E0,0x200)]
INSTALLED = [0x08008AE1,0x08008521,0x08008415,0x080088AD,0x08008A05]
ENDPOINTS = [(-32,-32),(58,-32),(144,-32),(240,-32),(-32,44),(240,28),
             (-32,86),(240,90),(-34,120),(50,160),(144,160),(210,150)]
CONTEXT = 0x020199A0
PARTICLES = 0x020021A0
STACK = 0x03002060
DUMMY = 0x03006101
INITIAL_PREFIX = 0x13579BDF


def pattern(seed,length):
    return struct.pack("<"+"I"*(length//4),*[(seed+index*0x01020304)&0xFFFFFFFF for index in range(length//4)])


def synthetic_asset(source,length):
    return bytes(((source>>1)+index*37)&255 for index in range(length))


def generate(path):
    oracle=control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    oracle.uc.mem_map(0x02000000,0x40000)
    oracle.uc.mem_map(0x06000000,0x20000)
    from unicorn import UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3,UC_ARM_REG_SP,UC_ARM_REG_LR,UC_ARM_REG_PC
    registers=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_R3]
    calls,requests,prefixes=[],[],[]
    full_sequence=False
    callbacks={start for name,(start,_end) in ROUTINES.items() if name in ["paletteFade","specialFade","timeoutFade","idle","localizedResources","scroll","wait","finish"]}

    def hook(cpu,address,_size,_data):
        offset=address-control.ROM_BASE
        if offset==0x8260:
            prefixes.append(struct.unpack("<I",cpu.mem_read(cpu.reg_read(UC_ARM_REG_SP)-0x4C,4))[0])
            return
        if offset in callbacks and full_sequence:
            calls.append({"callbackId":address+1,"arguments":[cpu.reg_read(UC_ARM_REG_R0)]})
            return
        if offset in ARITY:
            arguments=[cpu.reg_read(registers[index]) for index in range(min(ARITY[offset],4))]
            if ARITY[offset]==5:
                arguments.append(struct.unpack("<I",cpu.mem_read(cpu.reg_read(UC_ARM_REG_SP),4))[0])
            calls.append({"entryOffset":offset,"arguments":arguments})
            if offset==0x2580 and full_sequence:
                return
        elif offset==0x39A0:
            destination,source=[cpu.reg_read(registers[index]) for index in range(2)]
            raw=oracle.rom[source-control.ROM_BASE:]
            if raw[0]!=0x10:
                raise AssertionError("Title asset must be native type10")
            length=int.from_bytes(raw[1:4],"little")
            cpu.mem_write(destination,synthetic_asset(source,length))
            requests.append({"sourceAddress":source,"destination":destination,"length":length})
        elif offset==0x162C:
            destination,source,length=[cpu.reg_read(registers[index]) for index in range(3)]
            if length!=0x800:
                raise AssertionError("Unexpected title copy size")
            cpu.mem_write(destination,bytes(cpu.mem_read(source,length)))
        elif address==DUMMY-1:
            calls.append({"callbackId":DUMMY,"arguments":[cpu.reg_read(UC_ARM_REG_R0)]})
        else:
            return
        cpu.reg_write(UC_ARM_REG_R0,0)
        cpu.reg_write(UC_ARM_REG_PC,cpu.reg_read(UC_ARM_REG_LR))

    oracle.uc.hook_add(UC_HOOK_CODE,hook)
    initial_particles=[]
    for index,(x,y) in enumerate(ENDPOINTS):
        args=interpolation.arguments(120,80,x,y,1,1,0x086E7124 if index%2==0 else 0x086E712C)
        initial_particles.append(interpolation.construct(oracle,args,INITIAL_PREFIX))

    def source_resources(synthetic):
        for address,length in RESOURCE_SPANS:
            offset=address-control.ROM_BASE
            oracle.uc.mem_write(address,oracle.rom[offset:offset+length])
        if synthetic:
            oracle.uc.mem_write(0x0806A34C,struct.pack("<18I",*[1+index%3 for index in range(18)]))
            oracle.uc.mem_write(0x086E7128,b"\0\0")
            oracle.uc.mem_write(0x086E7130,b"\0\4")

    rng=random.Random(0x91C4)

    def new_case(**changes):
        item={"seed":rng.randrange(1<<32),"routine":"initialize","language":0,"mode":0,
              "mainParameter":3,"depth":5,"parameter":0,"contextFlag":0,"completion":0,
              "ready":0,"accepted":0,"blink":0,"idleTimer":0,"delay":0,"phase":0,
              "allComplete":False,"scrollIndex":0,"scrollDone":0,"vertical":160,
              "otherVertical":96,"pressed":0,"queueCount":rng.choice([0,120,127,128,255]),"angle":0}
        item.update(changes)
        return item

    def reset(case):
        seed=case["seed"]
        oracle.uc.mem_write(0x02000000,pattern(seed,0x40000))
        oracle.uc.mem_write(0x03000000,pattern(seed^0xA55A1234,0x8000))
        oracle.uc.mem_write(0x06000000,pattern(seed^0x7ACE1200,0x20000))
        for index,original in enumerate(initial_particles):
            value=bytearray(original)
            if case["allComplete"]:
                struct.pack_into("<H",value,0x1C,0xFFFF)
            oracle.uc.mem_write(PARTICLES+index*32,bytes(value))
        for address,value in [(0x03003060,case["language"]),(0x03000E00,1),(0x03000E2C,case["mainParameter"]),
                              (STACK,case["depth"]),(0x02002324,case["blink"]),(0x0200232C,case["idleTimer"]),
                              (0x02002338,case["scrollIndex"]),(0x0200233A,case["angle"]),
                              (0x0200233C,case["phase"]),(0x0200233E,case["delay"]),
                              (0x03000786,case["vertical"]),(0x03000788,case["otherVertical"]),(0x03000516,case["pressed"])]:
            oracle.uc.mem_write(address,struct.pack("<H",value))
        for address,value in [(0x030040B0,0x03002980),(0x03000E04,0x080091C5),
                              (0x02002320,CONTEXT),(CONTEXT,case["completion"]),(CONTEXT+4,case["contextFlag"]),
                              (0x02002330,case["ready"]),(0x02002328,case["accepted"]),(0x02002334,case["scrollDone"])]:
            oracle.uc.mem_write(address,struct.pack("<I",value))
        display=struct.unpack("<H",oracle.uc.mem_read(0x03000770,2))[0]
        oracle.uc.mem_write(0x03000770,struct.pack("<H",(display&~7)|case["mode"]))
        oracle.uc.mem_write(0x03000018,bytes([case["queueCount"]]))
        if case["depth"]:
            oracle.uc.mem_write(STACK+0x2C+(case["depth"]-1)*2,struct.pack("<H",case["parameter"]))
            oracle.uc.mem_write(STACK+4+(case["depth"]-1)*4,struct.pack("<I",DUMMY))
        calls.clear();requests.clear();prefixes.clear()

    def snapshot():
        return {"ewramSha256":hashlib.sha256(oracle.uc.mem_read(0x02000000,0x40000)).hexdigest(),
                "iwramWithoutStackSha256":hashlib.sha256(oracle.uc.mem_read(0x03000000,0x7800)).hexdigest(),
                "vramTransactionWindowSha256":hashlib.sha256(oracle.uc.mem_read(0x06000000,0x20000)).hexdigest()}

    source_resources(True)
    inputs=[]
    for routine in ["initialize","resume","restart"]:
        for mode in range(8):
            for lang in [0,1,6]:
                inputs.append(new_case(routine=routine,mode=mode,language=lang))
    for routine in ["paletteFade","specialFade","timeoutFade","finish","wait"]:
        for parameter in [0,1,31,32,0x7FFF,0x8000,0xFFFF]:
            for depth in [1,10]:
                for pressed,blocked in [(0,0),(8,0),(8,1)]:
                    inputs.append(new_case(routine=routine,parameter=parameter,depth=depth,pressed=pressed,contextFlag=blocked))
    for lang in [0,1,2,3,4,5,6,0xFFFF]:
        inputs.append(new_case(routine="localizedResources",language=lang))
    for vertical in [0,4,160,0xFFFF]:
        for index in [0,17]:
            for pressed in [0,1,8,10]:
                inputs.append(new_case(routine="scroll",vertical=vertical,scrollIndex=index,pressed=pressed))
    for delay in [0,89,90]:
        for phase in [0,1,2]:
            for complete in [False,True]:
                for pressed in [0,8,2,10]:
                    inputs.append(new_case(routine="idle",delay=delay,phase=phase,allComplete=complete,
                                           pressed=pressed,idleTimer=rng.choice([1319,1320]),contextFlag=rng.choice([0,1])))
    for ready in [0,1,2]:
        for accepted in [0,1,2]:
            for blink in [0,15,16,31,32,0xFFFF]:
                for depth in [0,1,10]:
                    inputs.append(new_case(routine="update",ready=ready,accepted=accepted,blink=blink,depth=depth,idleTimer=0xFFFF))
    for parameter in [0,1,2,3,0xFFFF]:
        for completion in [0,1]:
            inputs.append(new_case(routine="entry",mainParameter=parameter,completion=completion))
    cases=[]
    for item in inputs:
        reset(item)
        oracle.call(item["routine"],CONTEXT)
        if prefixes and len(set(prefixes))!=1:
            raise AssertionError("Constructor prefix varies within one title invocation")
        cases.append({**item,"constructorPrefix":prefixes[0] if prefixes else None,"constructorCount":len(prefixes),
                      "calls":calls.copy(),"resourceRequests":requests.copy(),**snapshot()})

    source_resources(False)
    full_sequence=True
    initial=new_case(routine="entry",mainParameter=0,queueCount=0)
    reset(initial)
    oracle.call("entry")
    initial_result={"constructorPrefix":prefixes[0],"calls":calls.copy(),"resourceRequests":requests.copy(),**snapshot()}
    ticks=[]
    for frame in range(512):
        pressed=8 if frame==220 else 0
        oracle.uc.mem_write(0x03000516,struct.pack("<H",pressed))
        calls.clear();requests.clear();prefixes.clear()
        oracle.call("entry")
        if prefixes and len(set(prefixes))!=1:
            raise AssertionError("Idle reconstruction uses multiple stack-prefix words")
        ticks.append({"pressed":pressed,"constructorPrefix":prefixes[0] if prefixes else None,
                      "calls":calls.copy(),"resourceRequests":requests.copy(),**snapshot()})
        main=struct.unpack("<I",oracle.uc.mem_read(0x03000E04,4))[0]
        if main!=0x080091C5:
            break
    else:
        raise AssertionError("Title Start sequence exceeded512 ticks")
    if main!=0x0801CF4D:
        raise AssertionError("Expected native mode-selection transition")
    assets=[]
    for address in ASSETS:
        offset=address-control.ROM_BASE
        decoded,consumed=reference_parity.decode(oracle.rom[offset:],maximum=0x20000)
        assets.append({"sourceAddress":address,"storedLength":consumed,"storedSha256":hashlib.sha256(oracle.rom[offset:offset+consumed]).hexdigest(),
                       "decodedLength":len(decoded),"decodedSha256":hashlib.sha256(decoded).hexdigest()})
    old=interpolation.ROUTINES
    try:
        interpolation.ROUTINES={"constructor":(0x8260,0x8358),"update":(0x8358,0x8414),"submission":(0x2614,0x2690)}
        copies=interpolation.generate(path)
    finally:
        interpolation.ROUTINES=old
    existing=json.loads(spp_vectors.path("interpolation-vectors.json").read_text(encoding="utf8"))
    if copies["constructors"]!=existing["constructors"] or copies["updates"]!=existing["updates"]:
        raise AssertionError("Title interpolation copies differ from completed Rust contract")
    return {"schemaVersion":1,"romSha256":control.ROM_SHA256,
            "scope":"Original title state/control,8260/8358 and2614 execute; listed frame/affine/palette/audio calls intercepted;39A0 writes synthetic same-length data and162C copies supplied RAM; default scroll/priorities synthetic; local sequence uses native table/priorities; no audio/PPU/full mode-screen acceptance",
            "routines":{name:{"offset":start,"length":end-start,"sha256":hashlib.sha256(oracle.rom[start:end]).hexdigest()} for name,(start,end) in ROUTINES.items()},
            "resourceSpans":[{"address":address,"length":length,"sha256":hashlib.sha256(oracle.rom[address-control.ROM_BASE:address-control.ROM_BASE+length]).hexdigest()} for address,length in RESOURCE_SPANS],
            "authenticAssets":assets,"initialParticlePrefix":INITIAL_PREFIX,"cases":cases,
            "scalarResources":{"backgroundDestinations":list(struct.unpack_from("<8I",oracle.rom,0x66C54)),"titleAudioPlayer":struct.unpack_from("<I",oracle.rom,0x6DD878)[0]},
            "interpolationCopyParity":{"constructors":len(copies["constructors"]),"updateOperations":sum(len(case["steps"]) for case in copies["updates"]),"existingFixtureSha256":hashlib.sha256(spp_vectors.path("interpolation-vectors.json").read_bytes()).hexdigest()},
            "nativeTableStartSequence":{"initial":initial,"initialized":initial_result,"ticks":ticks,"nextCallback":main}}


class TitleReferenceTests(unittest.TestCase):
    def test_fixture_declares_exact_transition_and_copy_parity(self):
        data=json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(data["romSha256"],control.ROM_SHA256)
        self.assertEqual(data["nativeTableStartSequence"]["nextCallback"],0x0801CF4D)
        self.assertEqual(data["interpolationCopyParity"]["constructors"],212)
        self.assertEqual(len(data["authenticAssets"]),17)

    @unittest.skipUnless(os.environ.get("SPP_ROM"),"Requires local SPP_ROM and Python unicorn")
    def test_actual_title_routines_regenerate_fixture(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]),json.loads(FIXTURE.read_text(encoding="utf8")))


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom",default=os.environ.get("SPP_ROM"));parser.add_argument("--write",type=Path)
    args=parser.parse_args()
    if not args.rom:parser.error("--rom or SPP_ROM is required")
    data=generate(args.rom)
    if args.write:args.write.write_text(json.dumps(data,separators=(",",":"))+"\n",encoding="utf8")
    elif data!=json.loads(FIXTURE.read_text(encoding="utf8")):raise SystemExit("Native title fixture mismatch")
    print(json.dumps({"verified":True,"cases":len(data["cases"]),"startSequenceTicks":len(data["nativeTableStartSequence"]["ticks"]),"interpolationCopyParity":data["interpolationCopyParity"]}))
