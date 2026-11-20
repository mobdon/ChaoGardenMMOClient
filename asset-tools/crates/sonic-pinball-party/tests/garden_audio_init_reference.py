#!/usr/bin/env python3
"""Native Garden audio init oracle; register image/10AE0 are explicit seams.

Executes105E8,109E8,108A4,10B84,10C70,10CEC and native player-open/clear.
BIOS CpuSet is a bounded exact word-copy/fill model. No rendered-audio claim.
Native copied code is hashed, never written into the public fixture.
"""
import argparse
import json
import os
from pathlib import Path
import struct

import reference_parity as parity
import spp_vectors  # noqa: E402

FIXTURE = spp_vectors.path("garden-audio-init-vectors.json")
EWRAM, IWRAM, MMIO = 0x02000000, 0x03000000, 0x04000000
INFO, PSG, SIGNATURE = IWRAM + 0x61A0, IWRAM + 0x71E0, 0x68736D53
RETURN = IWRAM + 0x7E00
SOURCES = {
    "initializer": (0x105E8, 0x10660), "soundInit": (0x109E8, 0x10AE0),
    "psgInit": (0x108A4, 0x109BC), "soundMode": (0x10B84, 0x10C1C),
    "disable": (0x10C70, 0x10CEC), "enable": (0x10CEC, 0x10D28),
    "callbackInit": (0xFDC8, 0xFDF8), "callbackTable": (0x191C4, 0x19254),
    "copiedCode": (0xF9BC, 0xFDBC), "playerOpen": (0x10D28, 0x10DA0),
    "clearPlayer": (0xFD60, 0xFD78), "clearCallback": (0x109D4, 0x109E8),
    "biosCpuSet": (0x11AE4, 0x11AE8), "frequencyBoundary": (0x10AE0, 0x10B84),
    "mixerWrapper": (0x10660, 0x1066C), "mixerBoundary": (0xF938, 0xF9BC),
}


def pattern(seed, size):
    return bytes((seed + i * 37) & 255 for i in range(size))


def specifications():
    cases = []

    def add(name, entry, args=None, **extra):
        case = dict(name=name, entry=entry, args=args or [], seed=83,
                    signature=SIGNATURE, dma1=0, dma2=0, bias=0xA5, mutations=[])
        case.update(extra)
        cases.append(case)

    for dma1, dma2, bias in [(0, 0, 0), (0x02000000, 0, 255),
                              (0, 0xFFFFFFFF, 0x40), (0xFFFFFFFF, 0xFFFFFFFF, 0xA5)]:
        add(f"initialize-{dma1:x}-{dma2:x}-{bias}", 0x105E8, dma1=dma1, dma2=dma2, bias=bias)
        add(f"sound-init-{dma1:x}-{dma2:x}-{bias}", 0x109E8, [INFO], dma1=dma1, dma2=dma2, bias=bias)
    add("sound-init-alternate-buffer", 0x109E8, [IWRAM + 0x5000])
    for signature in (SIGNATURE, SIGNATURE + 1, 0):
        add(f"psg-init-{signature:x}", 0x108A4, [PSG], signature=signature)
    for mode in (0, 1, 0x80, 0xFF, 0x100, 0xF00, 0x1000, 0xF000,
                 0x10000, 0xF0000, 0x100000, 0x200000, 0x800000, 0xB00000,
                 0xFFFFFFFF, 0x0093F400):
        add(f"sound-mode-{mode:x}", 0x10B84, [mode])
    add("sound-mode-invalid-signature", 0x10B84, [0xFFFFFFFF], signature=SIGNATURE + 1)
    for signature in (SIGNATURE - 1, SIGNATURE, SIGNATURE + 1, SIGNATURE + 10, 0, 0xFFFFFFFF):
        for entry in (0x10C70, 0x10CEC):
            add(f"control-{entry:x}-{signature:x}", entry, signature=signature,
                dma1=0x02000000, dma2=0xFFFFFFFF)
    for entry, args in [(0x105E8, []), (0x109E8, [INFO]), (0x108A4, [PSG]),
                        (0x10B84, [0xFFFFFFFF]), (0x10C70, []), (0x10CEC, [])]:
        calls = {0x105E8: 32, 0x109E8: 15, 0x108A4: 10, 0x10B84: 9,
                 0x10C70: 6, 0x10CEC: 2}[entry]
        for ordinal in range(1, calls + 1):
            add(f"interrupt-{entry:x}-{ordinal}", entry, args,
                signature=SIGNATURE + 10 if entry == 0x10CEC else SIGNATURE,
                dma1=0x02000000, dma2=0xFFFFFFFF, abortAtCall=ordinal)
    add("sound-mode-live-info-slot", 0x10B84, [0x0093F400],
        mutations=[dict(call=1, address=IWRAM + 0x7FF0, width=4, value=IWRAM + 0x5000),
                   dict(call=1, address=IWRAM + 0x5000, width=4, value=SIGNATURE)])
    add("init-frequency-mutates-flags", 0x105E8,
        frequencyMutations=[dict(ordinal=2, address=EWRAM + 0x19BEA, width=2, value=0xABCD)])
    add("mixer-wrapper", 0x10660)
    add("mixer-wrapper-interrupt", 0x10660, abortAtCall=1)
    return cases


def generate(path):
    from unicorn import (Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE,
                         UC_HOOK_MEM_READ, UC_HOOK_MEM_WRITE, UC_MEM_READ)
    from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2,
                                   UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_SP)
    rom = parity.bounded_read(Path(path), parity.ROM_LENGTH)
    parity.validate_rom(rom)
    payload, consumed = parity.decode(rom[parity.GARDEN_OFFSET:], parity.GARDEN_LENGTH)
    parity.require(consumed == parity.GARDEN_STORED, "Garden stored length mismatch")
    parity.require(parity.sha256(payload) == parity.GARDEN_SHA256, "Garden identity mismatch")
    callback_values = list(struct.unpack_from("<36I", payload, 0x191C4))
    players = [list(struct.unpack_from("<IIBBH", payload, 0x19BE0 + i * 12)) for i in range(4)]
    cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
    cpu.mem_map(EWRAM, 0x40000)
    cpu.mem_map(IWRAM, 0x8000)
    cpu.mem_map(MMIO, 0x1000)
    state = {}

    def write(address, value, width=4):
        cpu.mem_write(address, value.to_bytes(width, "little"))

    def event(call):
        state["calls"].append(call)
        ordinal = len(state["calls"])
        for mutation in state["case"]["mutations"]:
            if mutation["call"] == ordinal:
                write(mutation["address"], mutation["value"], mutation["width"])
        if ordinal == state["case"].get("abortAtCall"):
            state["aborted"] = True
            cpu.emu_stop()
        return state["aborted"]

    def register(machine, access, address, size, value, _user):
        if access == UC_MEM_READ:
            # Mutations occur at the boundary before the returned read value.
            call = dict(kind="read", address=address, width=size)
            event(call)
            call["value"] = int.from_bytes(machine.mem_read(address, size), "little")
        else:
            event(dict(kind="write", address=address, width=size, value=value))

    def frequency(machine, _address, _size, _user):
        state["frequency"] += 1
        interrupted = event(dict(kind="frequency", mode=machine.reg_read(UC_ARM_REG_R0)))
        for mutation in state["case"].get("frequencyMutations", []):
            if mutation["ordinal"] == state["frequency"]:
                write(mutation["address"], mutation["value"], mutation["width"])
        if not interrupted:
            machine.reg_write(UC_ARM_REG_PC, machine.reg_read(UC_ARM_REG_LR))

    def cpuset(machine, _address, _size, _user):
        source = machine.reg_read(UC_ARM_REG_R0)
        destination = machine.reg_read(UC_ARM_REG_R1)
        control = machine.reg_read(UC_ARM_REG_R2)
        assert control in (0x04000100, 0x050003EC, 0x05000040, 0x05000318)
        words = control & 0x1FFFFF
        assert words <= 0x3EC and source % 4 == 0 and destination % 4 == 0
        for index in range(words):
            at = source if control & 0x01000000 else source + index * 4
            machine.mem_write(destination + index * 4, bytes(machine.mem_read(at, 4)))
        machine.reg_write(UC_ARM_REG_PC, machine.reg_read(UC_ARM_REG_LR))

    def mixer(machine, _address, _size, _user):
        if not event(dict(kind="mixer", entry=0xF938)):
            machine.reg_write(UC_ARM_REG_PC, machine.reg_read(UC_ARM_REG_LR))

    cpu.hook_add(UC_HOOK_MEM_READ | UC_HOOK_MEM_WRITE, register, begin=MMIO, end=MMIO + 0xFFF)
    cpu.hook_add(UC_HOOK_CODE, frequency, begin=EWRAM + 0x10AE0, end=EWRAM + 0x10AE0)
    cpu.hook_add(UC_HOOK_CODE, cpuset, begin=EWRAM + 0x11AE4, end=EWRAM + 0x11AE4)
    cpu.hook_add(UC_HOOK_CODE, mixer, begin=EWRAM + 0xF938, end=EWRAM + 0xF938)
    cases = specifications()
    for case in cases:
        state.clear()
        state.update(case=case, calls=[], aborted=False, frequency=0)
        cpu.mem_write(EWRAM, payload)
        cpu.mem_write(IWRAM, pattern(case["seed"], 0x8000))
        cpu.mem_write(MMIO, pattern(71, 0x1000))
        write(IWRAM + 0x7FF0, INFO)
        write(INFO, case["signature"])
        write(MMIO + 0xC4, case["dma1"])
        write(MMIO + 0xD0, case["dma2"])
        write(MMIO + 0x89, case["bias"], 1)
        for register_id, value in zip((UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2), case["args"]):
            cpu.reg_write(register_id, value)
        cpu.reg_write(UC_ARM_REG_SP, IWRAM + 0x7F00)
        cpu.reg_write(UC_ARM_REG_LR, RETURN | 1)
        cpu.emu_start(EWRAM + case["entry"] + 1, RETURN, count=200000)
        assert state["aborted"] or cpu.reg_read(UC_ARM_REG_PC) == RETURN, case["name"]
        ram = bytes(cpu.mem_read(IWRAM, 0x7400))
        if "abortAtCall" in case:
            assert state["aborted"], (case["name"], len(state["calls"]))
        case.update(calls=state["calls"], aborted=state["aborted"], ramSha256=parity.sha256(ram),
                    copiedCodeSha256=parity.sha256(ram[0x3E44:0x4244]))
        if not state["aborted"]:
            case["mmioSha256"] = parity.sha256(cpu.mem_read(MMIO, 0x1000))
    return dict(romSha256=parity.sha256(rom), payloadSha256=parity.sha256(payload),
                nativeInstructionParity=True, hardwareModel="register-image-and-10AE0-boundary",
                biosModel="bounded-native-CpuSet-word-copy-fill", callbackValues=callback_values,
                players=players, snapshot=dict(base=IWRAM, length=0x7400),
                frequencyTimerScanlineValidated=False, renderedAudioValidated=False,
                mixerValidated=False, sequencerValidated=False, gardenEndToEndValidated=False,
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
                       "Native Garden audio-init vectors differ")
    print(json.dumps(dict(cases=len(data["cases"]), frequencyTimerScanlineValidated=False)))
