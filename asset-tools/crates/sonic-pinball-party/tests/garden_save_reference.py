#!/usr/bin/env python3
"""Native A3VE save-control oracle with explicit synthetic hardware seams.

Runs original0248/03AC/043C, CRC and RNG instructions. Flash/timer/audio/VBlank
calls are recorded synthetic boundaries, not real flash hardware acceptance.
Only synthetic inputs, hashes and scalar source metadata are written.
"""

import argparse
import json
import os
from pathlib import Path
import struct

import reference_parity as parity
import spp_vectors  # noqa: E402


FIXTURE = spp_vectors.path("garden-save-vectors.json")
EWRAM, IWRAM = 0x02000000, 0x03000000
SAVE, LENGTH = IWRAM + 0x4AA0, 0x858
SNAPSHOT_LENGTH = 0x7400
RETURN = IWRAM + 0x7E00
SOURCES = {
    "loader": (0x248, 0x3AC),
    "writer": (0x3AC, 0x43C),
    "initializer": (0x43C, 0x480),
    "rngGetSet": (0x151C, 0x1534),
    "rng": (0x1534, 0x1554),
    "boundedRandom": (0x1554, 0x1588),
    "crc": (0x185C, 0x18A0),
    "initialDelay": (0x8810, 0x882C),
    "biosDivRemainder": (0x11AEC, 0x11AF4),
    "crcTable": (0x1F4E0, 0x1F6E0),
}
HOOKS = {0x12028: 0, 0x11C10: 2, 0x11DA0: 4, 0x11FE0: 3,
         0x107A0: 0, 0x10660: 0, 0x11B04: 0, 0x10C70: 0, 0x10CEC: 0}


def pattern(seed, size):
    return bytes((seed + i * 37) & 255 for i in range(size))


def checksum(data):
    value = 0xFFFF
    for byte in data:
        value ^= byte
        for _ in range(8):
            value = (value >> 1) ^ (0x8408 if value & 1 else 0)
    return value ^ 0xFFFF


def record(spec):
    data = bytearray(pattern(spec.get("seed", 19), LENGTH))
    struct.pack_into("<H", data, 2, 0x1965)
    for field, offset in [("generation", 4), ("credits", 8), ("rings", 12), ("rng", 24)]:
        struct.pack_into("<I", data, offset, spec.get(field, 0))
    struct.pack_into("<H", data, 0, checksum(data[2:]))
    if spec.get("invalid") == "marker":
        data[2] ^= 1
    elif spec.get("invalid") == "checksum":
        data[-1] ^= 0x80
    return data


def specifications():
    cases = []

    def add(name, steps, **extra):
        value = dict(name=name, steps=steps, ramSeed=83, programCredits=0,
                     initial=dict(sectorIndex=0, generation=11, rng=0x12345678,
                                  workingRings=125, record=dict(seed=31, generation=8,
                                                             credits=0, rings=25, rng=7)))
        value.update(extra)
        cases.append(value)

    for seed in (0, 1, 0xFFFFFFFF, 0x54696368):
        add(f"initialize-{seed}", [dict(routine="initialize")],
            initial=dict(sectorIndex=6, generation=0, rng=seed, workingRings=90,
                         record=dict(seed=seed & 255)))
    for credits in (0, 100, 99999, 0xFFFFFFFF):
        add(f"load-empty-{credits}", [dict(routine="load")], programCredits=credits)
    add("load-first-valid-zero", [dict(routine="load")],
        flashRecords=[dict(slot=3, generation=0, credits=50, rings=17, rng=123, seed=71)])
    add("load-ties-first", [dict(routine="load")], flashRecords=[
        dict(slot=i, generation=99, credits=0, rings=i * 100, rng=i + 9, seed=20 + i)
        for i in range(6)])
    add("load-unsigned-generation", [dict(routine="load")], flashRecords=[
        dict(slot=0, generation=0x7FFFFFFF, rings=10),
        dict(slot=1, generation=0xFFFFFFFF, rings=20),
        dict(slot=2, generation=0, rings=30),
        dict(slot=5, generation=0x80000000, rings=40)])
    add("load-corrupt-newer", [dict(routine="load")], flashRecords=[
        dict(slot=0, generation=8, rings=81, rng=811),
        dict(slot=4, generation=0xFFFFFFFF, invalid="checksum"),
        dict(slot=5, generation=0xFFFFFFFE, invalid="marker")])
    add("load-all-corrupt-preserve-tail", [dict(routine="load")], flashRecords=[
        dict(slot=i, generation=i, seed=10 + i, invalid="checksum" if i & 1 else "marker")
        for i in range(6)])
    for name, credits, program, rings in [
        ("less", 200, 100, 200000), ("equal", 99, 99, 0xFFFFFFFF),
        ("add", 10, 40, 42), ("cap", 10, 500000, 20),
        ("overflow", 0, 0xFFFFFFFF, 256), ("wrap-to-zero", 0, 1, 0xFFFFFFFF),
    ]:
        add("load-credits-" + name, [dict(routine="load")], programCredits=program,
            flashRecords=[dict(slot=5, generation=5, credits=credits, rings=rings,
                               rng=0xCAFEBABE, seed=77)], readResult=0xFFFF0001,
            identifyResult=1, timerResult=1)
    for index in (0, 4, 5, 6, 255):
        add(f"write-sector-{index}", [dict(routine="write")],
            initial=dict(sectorIndex=index, generation=0xFFFFFFFF, rng=0xFFFFFFFF,
                         workingRings=0xFFFFFFFF, record=dict(seed=53)))
    for failures in range(1, 6):
        plans = [dict(result=0xC000 + i, prefix=0, erase=False) for i in range(failures)]
        if failures < 5:
            plans.append(dict(result=0, prefix=LENGTH, erase=True))
        add(f"write-retries-{failures}", [dict(routine="write")], writePlans=plans)
    add("write-torn-then-success", [dict(routine="write")], writePlans=[
        dict(result=0xC001, prefix=0x100, erase=True),
        dict(result=0x80000000, prefix=LENGTH - 1, erase=True),
        dict(result=0, prefix=LENGTH, erase=True)])
    add("write-boundary-mutations", [dict(routine="write")], writePlans=[
        dict(result=1, prefix=0, erase=False), dict(result=0, prefix=LENGTH, erase=True)],
        mutations=[dict(call=2, address=SAVE + 0x140, width=4, value=0xDEC0DE01),
                   dict(call=5, address=IWRAM + 0x80, width=1, value=4)])
    all_fail = [dict(result=1, prefix=0, erase=False)] * 5
    for call in range(1, 12):
        add(f"write-interrupt-call-{call}", [dict(routine="write")],
            writePlans=all_fail, abortAtCall=call)
    for ordinal, prefix in [(1, 0), (3, 1), (6, LENGTH - 1), (7, 24), (7, LENGTH)]:
        add(f"load-interrupt-read-{ordinal}-{prefix}", [dict(routine="load")],
            flashRecords=[dict(slot=1, generation=3, rings=123, rng=456, seed=91)],
            abortRead=dict(ordinal=ordinal, prefix=prefix))
    for prefix in (0, 1, 24, 0x100, LENGTH - 1, LENGTH):
        add(f"interrupted-write-fresh-load-{prefix}",
            [dict(routine="write"), dict(routine="load", resetRamSeed=117)],
            flashRecords=[dict(slot=0, generation=10, credits=0, rings=70, rng=700, seed=18)],
            writePlans=[dict(result=1, prefix=prefix, erase=True, abort=True)])
    return cases


def generate(path):
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
    from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2,
                                   UC_ARM_REG_R3, UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_SP)
    rom = parity.bounded_read(Path(path), parity.ROM_LENGTH)
    parity.validate_rom(rom)
    payload, consumed = parity.decode(rom[parity.GARDEN_OFFSET:], parity.GARDEN_LENGTH)
    parity.require(consumed == parity.GARDEN_STORED, "Garden stored length mismatch")
    parity.require(parity.sha256(payload) == parity.GARDEN_SHA256, "Garden identity mismatch")
    cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
    cpu.mem_map(EWRAM, 0x40000)
    cpu.mem_map(IWRAM, 0x8000)
    cpu.mem_write(EWRAM, payload)
    registers = [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2, UC_ARM_REG_R3]
    state = {}

    def stop():
        state["aborted"] = True
        cpu.emu_stop()

    def boundary(machine, address, _size, _user):
        offset = address - EWRAM
        args = [machine.reg_read(r) for r in registers[:HOOKS[offset]]]
        call = dict(entry=offset, args=args)
        state["calls"].append(call)
        ordinal = len(state["calls"])
        for mutation in state["case"].get("mutations", []):
            if mutation["call"] == ordinal:
                machine.mem_write(mutation["address"], mutation["value"].to_bytes(mutation["width"], "little"))
        if ordinal == state["case"].get("abortAtCall"):
            call["aborted"] = True
            stop()
            return
        result = 0
        if offset == 0x12028:
            result = state["case"].get("identifyResult", 0)
        elif offset == 0x11C10:
            assert args == [2, IWRAM + 0x4A88]
            result = state["case"].get("timerResult", 0)
        elif offset == 0x11DA0:
            sector, start, destination, size = args
            assert 10 <= sector <= 15 and start == 0 and destination == SAVE and size == LENGTH
            state["reads"] += 1
            prefix = size
            abort = state["case"].get("abortRead", {})
            interrupted = state["reads"] == abort.get("ordinal")
            if interrupted:
                prefix = abort["prefix"]
            source = bytes(state["flash"][sector * 0x1000:sector * 0x1000 + prefix])
            machine.mem_write(destination, source)
            if interrupted:
                call["aborted"] = True
                stop()
                return
            result = state["case"].get("readResult", 0)
        elif offset == 0x11FE0:
            sector, source, size = args
            assert 10 <= sector <= 15 and source == SAVE and size == LENGTH
            plans = state["case"].get("writePlans", [])
            index = state["writes"]
            plan = plans[index] if index < len(plans) else dict(result=0, prefix=LENGTH, erase=True)
            state["writes"] += 1
            data = bytes(machine.mem_read(source, size))
            call["recordSha256"] = parity.sha256(data)
            at = sector * 0x1000
            if plan.get("erase"):
                state["flash"][at:at + 0x1000] = bytes([255]) * 0x1000
            prefix = plan["prefix"]
            assert 0 <= prefix <= size
            state["flash"][at:at + prefix] = data[:prefix]
            if plan.get("abort"):
                call["aborted"] = True
                stop()
                return
            result = plan["result"]
        call["result"] = result
        machine.reg_write(UC_ARM_REG_R0, result)
        machine.reg_write(UC_ARM_REG_PC, machine.reg_read(UC_ARM_REG_LR))

    def bios(machine, interrupt, _user):
        assert interrupt == 2 and machine.reg_read(UC_ARM_REG_PC) == EWRAM + 0x11AEE
        a, b = (machine.reg_read(r) for r in registers[:2])
        assert 0 <= a < 32768 and b == 30
        machine.reg_write(UC_ARM_REG_R0, a // b)
        machine.reg_write(UC_ARM_REG_R1, a % b)
        machine.reg_write(UC_ARM_REG_R3, a // b)

    cpu.hook_add(UC_HOOK_INTR, bios)
    for offset in HOOKS:
        cpu.hook_add(UC_HOOK_CODE, boundary, begin=EWRAM + offset, end=EWRAM + offset)

    def reset_ram(case, seed):
        cpu.mem_write(IWRAM, pattern(seed, 0x8000))
        cpu.mem_write(EWRAM + 8, struct.pack("<I", case["programCredits"]))
        init = case["initial"]
        cpu.mem_write(SAVE, bytes(record(init["record"])))
        cpu.mem_write(IWRAM + 0x80, bytes([init["sectorIndex"]]))
        for key, offset in [("generation", 0x84), ("rng", 0x13A8), ("workingRings", 0x6194)]:
            cpu.mem_write(IWRAM + offset, struct.pack("<I", init[key]))

    cases = specifications()
    for case in cases:
        state.clear()
        state.update(case=case, flash=bytearray([255]) * 0x10000, calls=[], reads=0, writes=0)
        for spec in case.get("flashRecords", []):
            at = (10 + spec["slot"]) * 0x1000
            state["flash"][at:at + LENGTH] = record(spec)
        reset_ram(case, case["ramSeed"])
        outcomes = []
        for step in case["steps"]:
            if "resetRamSeed" in step:
                reset_ram(case, step["resetRamSeed"])
            state["aborted"] = False
            cpu.reg_write(UC_ARM_REG_SP, IWRAM + 0x7F00)
            cpu.reg_write(UC_ARM_REG_LR, RETURN | 1)
            start = dict(load=0x248, write=0x3AC, initialize=0x43C)[step["routine"]]
            cpu.emu_start(EWRAM + start + 1, RETURN, count=2000000)
            assert state["aborted"] or cpu.reg_read(UC_ARM_REG_PC) == RETURN
            outcomes.append(dict(aborted=state["aborted"],
                                 ramSha256=parity.sha256(cpu.mem_read(IWRAM, SNAPSHOT_LENGTH)),
                                 flashSha256=parity.sha256(state["flash"])))
        case.update(outcomes=outcomes, calls=state["calls"])
    return dict(romSha256=parity.sha256(rom), payloadSha256=parity.sha256(payload),
                hardwareModel="synthetic-flash-timer-audio-vblank-boundaries",
                actualFlashHardwareValidated=False, canonicalServiceValidated=False,
                snapshot=dict(base=IWRAM, length=SNAPSHOT_LENGTH),
                sources={key: dict(space="garden", offset=a, length=z-a,
                                   sha256=parity.sha256(payload[a:z])) for key, (a,z) in SOURCES.items()},
                cases=cases)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    if not args.rom:
        parser.error("provide --rom or SPP_ROM")
    data = generate(args.rom)
    if args.write:
        parity.require(args.write.resolve() == FIXTURE.resolve(), "write only this synthetic fixture")
        FIXTURE.write_text(json.dumps(data, separators=(",", ":")) + "\n", encoding="utf8")
    else:
        parity.require(data == json.loads(parity.bounded_read(FIXTURE, 2 * 1024 * 1024)),
                       "Native Garden save vectors differ")
    print(json.dumps(dict(cases=len(data["cases"]), actualFlashHardwareValidated=False)))
