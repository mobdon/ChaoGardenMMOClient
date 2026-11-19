"""Original A3VE mode-menu transition oracle with declared native boundaries.

Native1CF4C,195E0 selection, its five auxiliaries and re-entry wrappers execute;
large initializer/renderer, cache-copy, palette/save/RNG/BIOS/boot dependencies
are intercepted and recorded. Garden
control does not boot the payload. Its original local type10 resource is hashed
through an independent bounded decoder without embedding payload bytes.
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
import reference_parity

FIXTURE=Path(__file__).with_name("mode-menu-vectors.json")
ROUTINES={"entry":(0x1CF4C,0x1D23C),"resume":(0x1B69C,0x1B6A8),
          "resumeSecond":(0x1B6A8,0x1B6D0),"resumeThird":(0x1B6D0,0x1B6F8),
          "lookup":(0x1B6F8,0x1B73C),"selection":(0x195E0,0x19DFC),
          "selectionFade":(0x15E20,0x15E58),"slideOut":(0x15E58,0x15EA0),
          "slideIn":(0x15EA0,0x15EE8),"cancelFade":(0x15EE8,0x15F3C),
          "paletteIntro":(0x19DFC,0x19F78)}
ARITY={0x1A100:1,0x1A8F8:1,0x1AF84:1,0x1B73C:1,0x5784:3,0x49AF4:0,
       0x5F040:0,0x4973C:1,0x5FECC:2,0x36D4:1,0x5FF00:1,
       0x389C:1,0x5724:3,0x49BE8:0,0x162C:3,0x37D4:2}
CONTEXT=0x020199A0
CONTEXT_SLOT=0x02012BBC
LOOKUP=0x080AB29A


def pattern(seed,length):
    return struct.pack("<"+"I"*(length//4),*[(seed+index*0x01020304)&0xFFFFFFFF for index in range(length//4)])


def generate(path):
    oracle=control.NativeOracle(path)
    oracle.routines.update(ROUTINES)
    oracle.uc.mem_map(0x02000000,0x40000)
    from unicorn import UC_HOOK_CODE
    from unicorn.arm_const import UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2,UC_ARM_REG_LR,UC_ARM_REG_PC
    registers=[UC_ARM_REG_R0,UC_ARM_REG_R1,UC_ARM_REG_R2]
    calls=[]
    active=None

    def hook(cpu,address,_size,_data):
        offset=address-control.ROM_BASE
        if offset not in ARITY:
            return
        arguments=[cpu.reg_read(registers[index]) for index in range(ARITY[offset])]
        calls.append({"entryOffset":offset,"arguments":arguments})
        if offset==0x1B73C:
            cpu.mem_write(CONTEXT_SLOT,struct.pack("<I",active["afterUpdatePointer"]))
            cpu.mem_write(active["afterUpdatePointer"],struct.pack("<I",active["completion"]))
        result=active["randomResult"] if offset==0x36D4 else active.get("unlockResult",0) if offset==0x49BE8 else 0
        cpu.reg_write(UC_ARM_REG_R0,result)
        cpu.reg_write(UC_ARM_REG_PC,cpu.reg_read(UC_ARM_REG_LR))

    oracle.uc.hook_add(UC_HOOK_CODE,hook)
    rng=random.Random(0x1CF4C)

    def new_case(**changes):
        case={"seed":rng.randrange(1<<32),"routine":"entry","mainParameter":4,"depth":rng.choice([0,1,10]),
              "selection":0,"fade":31,"completion":1,"resumeSaved":0,"savedByte0":0,"savedByte1":0,
              "language":rng.randrange(8),"afterUpdatePointer":rng.choice([CONTEXT,0x0201A000]),"randomResult":rng.randrange(1<<32)}
        case.update(changes)
        return case

    def reset(case):
        oracle.uc.mem_write(0x02000000,pattern(case["seed"],0x40000))
        oracle.uc.mem_write(0x03000000,pattern(case["seed"]^0xA55A1234,0x8000))
        for address,value in[(0x03000E00,1),(0x03000E2C,case["mainParameter"]),(0x03002060,case["depth"]),
                            (0x02012C0A,case["selection"]),(0x02012C10,case["fade"]),(0x03003060,case["language"])]:
            oracle.uc.mem_write(address,struct.pack("<H",value))
        for address,value in[(0x03000E04,0x0801CF4D),(CONTEXT_SLOT,CONTEXT),(CONTEXT,0),
                            (0x02012C30,case["resumeSaved"]),(0x030040B0,0x03002980),(0x030040B4,0x02030000)]:
            oracle.uc.mem_write(address,struct.pack("<I",value))
        oracle.uc.mem_write(0x03003064,bytes([case["savedByte0"],case["savedByte1"]]))
        calls.clear()

    def snapshot():
        return{"ewramSha256":hashlib.sha256(oracle.uc.mem_read(0x02000000,0x40000)).hexdigest(),
               "iwramWithoutStackSha256":hashlib.sha256(oracle.uc.mem_read(0x03000000,0x7800)).hexdigest()}

    inputs=[]
    for parameter in[0,1,2,3]:
        inputs.append(new_case(mainParameter=parameter))
    for selected in list(range(8))+[0x8000,0xFFFF]:
        for fade in[0,30,31,32,0xFFFF]:
            for complete in[0,1]:
                for resume in[0,1]:
                    for first,second in[(0,0),(1,0),(1,2)]:
                        inputs.append(new_case(selection=selected,fade=fade,completion=complete,resumeSaved=resume,savedByte0=first,savedByte1=second))
    for language in[0,1,2,3,4,5,6,0xFFFF]:
        inputs.append(new_case(selection=4,language=language,randomResult=0xABCD1234))
    for parameter in[0xFFFF,0x8000]:
        inputs.append(new_case(mainParameter=parameter,selection=0))
    for routine in["resume","resumeSecond","resumeThird"]:
        inputs.append(new_case(routine=routine))
    cases=[]
    for case in inputs:
        active=case
        reset(case)
        oracle.call(case["routine"],CONTEXT)
        cases.append({**case,"calls":calls.copy(),**snapshot()})

    def selection_case(**changes):
        case=new_case(routine="selection",depth=5)
        case.update({"pressed":0,"held":0,"repeat":0,"movingLeft":0,"movingRight":0,
                     "accepted":0,"unlockResult":0,"mode":0,"parameter":0,
                     "paletteFrame":0,"slideScale":256,"slidePosition":256,
                     "leftScale":128,"middleScale":256,"rightScale":128,
                     "leftX":84,"middleX":120,"rightX":192,"edgeX":240,
                     "selection":0,"previous":6,"next":1,"incoming":2})
        case.update(changes)
        return case

    def reset_selection(case):
        reset(case)
        oracle.uc.mem_write(0x02012BC0,bytes(0x80))
        oracle.uc.mem_write(CONTEXT_SLOT,struct.pack("<I",CONTEXT))
        for address,value in[(0x03000516,case["pressed"]),(0x03000514,case["held"]),
                            (0x02012C3A,case["repeat"]),(0x02012C08,case["previous"]),
                            (0x02012C0A,case["selection"]),(0x02012C0C,case["next"]),(0x02012C0E,case["incoming"]),
                            (0x02012BC2,case["paletteFrame"]),(0x02012BE2,case["slideScale"]),
                            (0x02012BE4,case["slidePosition"]),(0x02012BCE,case["leftScale"]),
                            (0x02012BD0,case["middleScale"]),(0x02012BD2,case["rightScale"]),(0x02012BD4,128),
                            (0x02012BE6,case["leftX"]),(0x02012BE8,case["middleX"]),
                            (0x02012BEA,case["rightX"]),(0x02012BEC,case["edgeX"]),
                            (0x02012BEE,98),(0x02012BF0,86),(0x02012BF2,98),(0x02012BF4,98)]:
            oracle.uc.mem_write(address,struct.pack("<H",value))
        for address,value in[(0x02012BF8,case["movingLeft"]),(0x02012BFC,case["movingRight"]),(0x02031A40,case["accepted"])]:
            oracle.uc.mem_write(address,struct.pack("<I",value))
        display=struct.unpack("<H",oracle.uc.mem_read(0x03000770,2))[0]
        oracle.uc.mem_write(0x03000770,struct.pack("<H",(display&~7)|case["mode"]))
        depth=case["depth"]
        if depth:
            oracle.uc.mem_write(0x0300208C+(depth-1)*2,struct.pack("<H",case["parameter"]))
        calls.clear()

    selection_inputs=[]
    for selected in range(7):
        for pressed in[0,1,2,3]:
            for held in[0,0x10,0x20,0x30]:
                selection_inputs.append(selection_case(selection=selected,previous=(selected+6)%7,next=(selected+1)%7,
                                                      pressed=pressed,held=held,unlockResult=rng.choice([0,1,0xFFFFFFFF]),
                                                      savedByte0=rng.choice([0,1]),savedByte1=rng.choice([0,2]),
                                                      depth=rng.choice([1,2,9,10]),mode=rng.randrange(8)))
    for held in[0x10,0x20]:
        for repeat in[0,1,18,19,20,21,0xFFFF]:
            selection_inputs.append(selection_case(held=held,repeat=repeat))
    for direction in["movingLeft","movingRight"]:
        for scale in[0,128,240,256,0x7FFF,0x8000,0xFFFF]:
            for near_end in[False,True]:
                values={direction:1,"leftScale":scale,"rightScale":scale}
                if near_end:
                    values["leftX" if direction=="movingLeft" else "rightX"]=116 if direction=="movingLeft" else 128
                selection_inputs.append(selection_case(**values))
    for routine in["selectionFade","cancelFade"]:
        for parameter in[0,31,32,0x7FFF,0x8000,0xFFFF]:
            selection_inputs.append(selection_case(routine=routine,parameter=parameter))
    for routine in["slideIn","slideOut"]:
        for value in[0,1,31,32,224,255,256,0x7FFF,0x8000,0xFFFF]:
            selection_inputs.append(selection_case(routine=routine,slideScale=value))
    for frame in[0,9,10,19,20,29,30,39,40,49,50,59,60,69,70,79,80,0x7FFF,0x8000,0xFFFF]:
        selection_inputs.append(selection_case(routine="paletteIntro",paletteFrame=frame))
    selection_cases=[]
    for case in selection_inputs:
        active=case
        reset_selection(case)
        oracle.call(case["routine"],CONTEXT)
        selection_cases.append({**case,"calls":calls.copy(),**snapshot()})

    hold_sequences=[]
    for direction in[0x10,0x20]:
        case=selection_case(held=direction)
        active=case
        reset_selection(case)
        ticks=[]
        for frame in range(64):
            calls.clear()
            oracle.call("selection",CONTEXT)
            ticks.append({"calls":calls.copy(),**snapshot()})
        hold_sequences.append({"initial":case,"ticks":ticks})
    table=[0x100+index*17 for index in range(10)]
    oracle.uc.mem_write(LOOKUP,struct.pack("<10H",*table))
    lookups=[]
    for index,value in enumerate(table):
        oracle.uc.mem_write(0x02020000,struct.pack("<H",value))
        oracle.call("lookup",0x02020000)
        result=oracle.uc.reg_read(UC_ARM_REG_R0)
        if result!=([*range(8),16,17][index]):
            raise AssertionError("Unexpected original bounded lookup result")
        lookups.append({"input":value,"output":result})
    payload=0xD0970
    decoded,consumed=reference_parity.decode(oracle.rom[payload:],maximum=0x40000)
    palettes=set()
    for case in selection_cases:
        for call in case["calls"]:
            if call.get("entryOffset")==0x5724:
                palettes.add((call["arguments"][0],call["arguments"][2]*2))
    return{"schemaVersion":1,"romSha256":control.ROM_SHA256,
           "scope":"Original1CF4C,195E0 selection,15E20/15E58/15EA0/15EE8/19DFC auxiliaries,1B69C/1B6A8/1B6D0 and1B6F8 execute; large initialization/render,162C cache-copy,palette/save/RNG/BIOS/boot calls explicitly intercepted; Garden LZ77 bytes separately hashed, never executed; no native screen/ARM Garden acceptance",
           "routines":{name:{"offset":start,"length":end-start,"sha256":hashlib.sha256(oracle.rom[start:end]).hexdigest()} for name,(start,end)in ROUTINES.items()},
           "lookup":{"address":LOOKUP,"length":20,"sha256":hashlib.sha256(oracle.rom[LOOKUP-control.ROM_BASE:LOOKUP-control.ROM_BASE+20]).hexdigest(),"syntheticValues":table,"cases":lookups},
           "authenticGardenPayload":{"sourceAddress":control.ROM_BASE+payload,"storedLength":consumed,"storedSha256":hashlib.sha256(oracle.rom[payload:payload+consumed]).hexdigest(),"decodedLength":len(decoded),"decodedSha256":hashlib.sha256(decoded).hexdigest()},
           "paletteResources":[{"address":address,"length":length,"sha256":hashlib.sha256(oracle.rom[address-control.ROM_BASE:address-control.ROM_BASE+length]).hexdigest()} for address,length in sorted(palettes)],
           "backgroundLookup":{"address":0x08066C54,"length":32,"sha256":hashlib.sha256(oracle.rom[0x66C54:0x66C74]).hexdigest()},
           "cases":cases,"selectionCases":selection_cases,"holdSequences":hold_sequences,
           "backgroundDestinations":list(struct.unpack_from("<8I",oracle.rom,0x66C54))}


class ModeMenuReferenceTests(unittest.TestCase):
    def test_fixture_declares_native_and_boot_boundaries(self):
        data=json.loads(FIXTURE.read_text(encoding="utf8"))
        self.assertEqual(data["romSha256"],control.ROM_SHA256)
        self.assertEqual(len(data["lookup"]["cases"]),10)
        self.assertEqual(len(data["selectionCases"]),206)
        self.assertEqual(sum(len(sequence["ticks"]) for sequence in data["holdSequences"]),128)
        self.assertEqual(data["authenticGardenPayload"]["decodedLength"],188716)

    @unittest.skipUnless(os.environ.get("SPP_ROM"),"Requires local SPP_ROM and Python unicorn")
    def test_actual_wrappers_regenerate_fixture(self):
        self.assertEqual(generate(os.environ["SPP_ROM"]),json.loads(FIXTURE.read_text(encoding="utf8")))


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom",default=os.environ.get("SPP_ROM"));parser.add_argument("--write",type=Path)
    args=parser.parse_args()
    if not args.rom:parser.error("--rom or SPP_ROM is required")
    result=generate(args.rom)
    if args.write:args.write.write_text(json.dumps(result,separators=(",",":"))+"\n",encoding="utf8")
    elif result!=json.loads(FIXTURE.read_text(encoding="utf8")):raise SystemExit("Native mode-menu fixture mismatch")
    print(json.dumps({"verified":True,"wrapperCases":len(result["cases"]),"selectionCases":len(result["selectionCases"]),"heldTicks":sum(len(sequence["ticks"]) for sequence in result["holdSequences"]),"payloadDecodedLength":result["authenticGardenPayload"]["decodedLength"]}))
