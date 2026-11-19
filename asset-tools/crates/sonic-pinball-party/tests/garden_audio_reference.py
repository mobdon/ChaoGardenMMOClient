#!/usr/bin/env python3
"""Execute authentic Garden routing/lifecycle with synthetic sound-hardware seams.

No ROM, track code, samples or native art are published. Metadata, synthetic
inputs, complete boundary calls and bounded RAM hashes form the parity fixture.
"""
import argparse
import json
import os
from pathlib import Path
import struct

import reference_parity as parity

FIXTURE = Path(__file__).with_name("garden-audio-vectors.json")
EWRAM, IWRAM = 0x02000000, 0x03000000
SIGNATURE = 0x68736D53
INFO, HEADER, CHANNEL = IWRAM + 0x6100, EWRAM + 0x30000, IWRAM + 0x4000
RETURN = IWRAM + 0x7E00
SNAPSHOT_LENGTH = 0x7400
SOURCES = {
    "routing": (0x1066C, 0x1085C), "resumeAndFade": (0x105AC, 0x105E8),
    "open": (0x10D28, 0x10DA0), "start": (0x10DA0, 0x10E84),
    "stop": (0x10E84, 0x10EC4), "trackStop": (0x10240, 0x10284),
    "clearCallback": (0x109D4, 0x109E8), "clear64": (0xFD60, 0xFD78),
    "callbackTableInit": (0xFDC8, 0xFDF8), "callbackTable": (0x191C4, 0x19254),
    "players": (0x19BE0, 0x19C10), "songs": (0x19C10, 0x19D38),
    "highestSongIdCaller": (0x4BC6, 0x4C06),
}


def pattern(seed, size):
    return bytes((seed + i * 37) & 255 for i in range(size))


def specifications(players, songs):
    cases = []

    def add(name, entry, args=None, **extra):
        case = dict(name=name, entry=entry, args=args or [players[0][0], HEADER, 3],
                    seed=83, capacity=3, status=0x80000003, same=False,
                    signature=SIGNATURE, headerCount=3, headerPriority=4,
                    headerReverb=0, channels=[])
        case.update(extra)
        cases.append(case)

    for entry in (0x1066C, 0x10698, 0x106E4, 0x10738, 0x1076C):
        for song in range(37):
            add(f"route-{entry:x}-{song}", entry, [song, 0, 0], song=song,
                same=entry in (0x10738, 0x1076C))
    for entry in (0x10698, 0x106E4):
        for status in (0, 1, 0x80000000, 0x80000001, 0x10000, 0x7FFFFFFF):
            add(f"same-{entry:x}-{status:x}", entry, [11, 0, 0], song=11,
                same=True, status=status)
    add("route-low16", 0x1066C, [0xABCD0024, 0, 0], song=36)
    for entry in (0x107A0, 0x107D8, 0x105AC, 0x107CC):
        add(f"simple-{entry:x}", entry)
        add(f"invalid-signature-{entry:x}", entry, signature=SIGNATURE + 1)
    for entry in (0x105C8, 0x10804, 0x10814, 0x10834):
        for value in (0, 65535, 0xABCD1234):
            add(f"fade-{entry:x}-{value:x}", entry, [players[0][0], value, 0])
        add(f"fade-invalid-{entry:x}", entry, signature=0)
    for capacity in (0, 1, 16, 17, 255, 256, 257):
        add(f"open-{capacity}", 0x10D28, [players[0][0], players[0][1], capacity])
    add("open-invalid-info", 0x10D28, infoSignature=0)
    add("open-empty-previous", 0x10D28, previous=0)
    add("open-dynamic-clear", 0x10D28, clearTarget=0x02012491)
    add("open-dynamic-clear-abort", 0x10D28, clearTarget=0x02012491, abortAtCall=1)
    for capacity in (0, 1, 3, 16):
        for count in (0, 1, 3, 5, 255):
            add(f"start-{capacity}-{count}", 0x10DA0, capacity=capacity, headerCount=count)
    add("start-invalid-signature", 0x10DA0, signature=0)
    for status in (0, 1, 0x80000000, 0x80000001):
        for first in (0, 0x40):
            add(f"priority-{status:x}-{first}", 0x10DA0, priorityGate=1,
                status=status, playerPriority=5, firstFlag=first, same=True)
    add("priority-equal", 0x10DA0, priorityGate=1, status=1, playerPriority=4)
    add("start-reverb", 0x10DA0, headerReverb=0x91)
    add("start-reverb-abort", 0x10DA0, headerReverb=0xFF, abortAtCall=1)
    chain = [dict(active=1, kind=0), dict(active=0, kind=3), dict(active=1, kind=10)]
    for entry in (0x10240, 0x10DA0, 0x10E84, 0x107A0):
        args = [players[0][0], players[0][1], 0] if entry == 0x10240 else None
        add(f"channels-{entry:x}", entry, args, channels=chain, firstFlag=0x80)
        add(f"channels-abort-{entry:x}", entry, args, channels=chain,
            firstFlag=0x80, abortAtCall=1)
    add("channels-inactive-track", 0x10240, [players[0][0], players[0][1], 0], channels=chain)
    add("channels-next-reload", 0x10E84, channels=[dict(active=1, kind=1)], firstFlag=0x80,
        mutations=[dict(call=1, address=CHANNEL + 0x34, width=4, value=CHANNEL + 0x40),
                   dict(call=1, address=CHANNEL + 0x40, width=1, value=1),
                   dict(call=1, address=CHANNEL + 0x41, width=1, value=0),
                   dict(call=1, address=CHANNEL + 0x74, width=4, value=0)])
    for entry in (0x10DA0, 0x10E84):
        add(f"capacity-reload-{entry:x}", entry, channels=[dict(active=1, kind=1)],
            firstFlag=0x80, mutations=[dict(call=1, address=players[0][0] + 8, width=1, value=1)])
    add("header-reload-after-psg", 0x10DA0, channels=[dict(active=1, kind=1)], firstFlag=0x80,
        mutations=[dict(call=1, address=HEADER, width=1, value=1),
                   dict(call=1, address=HEADER + 8, width=4, value=0x0201D66D)])
    return cases


def generate(path):
    from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE
    from unicorn.arm_const import (UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2,
                                   UC_ARM_REG_PC, UC_ARM_REG_LR, UC_ARM_REG_SP)
    rom = parity.bounded_read(Path(path), parity.ROM_LENGTH)
    parity.validate_rom(rom)
    payload, consumed = parity.decode(rom[parity.GARDEN_OFFSET:], parity.GARDEN_LENGTH)
    parity.require(consumed == parity.GARDEN_STORED, "Garden stored length mismatch")
    parity.require(parity.sha256(payload) == parity.GARDEN_SHA256, "Garden identity mismatch")
    players = [list(struct.unpack_from("<IIB", payload, 0x19BE0 + i * 12)) for i in range(4)]
    songs = [list(struct.unpack_from("<IHH", payload, 0x19C10 + i * 8)) for i in range(37)]
    headers = []
    for address in sorted(set(song[0] for song in songs)):
        at = address - EWRAM
        count, _, priority, reverb, voices = struct.unpack_from("<4BI", payload, at)
        tracks = list(struct.unpack_from("<" + "I" * count, payload, at + 8))
        assert 0 <= count <= 16 and at + 8 + count * 4 <= len(payload)
        assert all(EWRAM <= track < EWRAM + len(payload) for track in tracks)
        if count:
            assert EWRAM <= voices <= EWRAM + len(payload) - 12
        headers.append(dict(address=address, count=count, priority=priority, reverb=reverb,
                            voices=voices, tracks=tracks))
    assert struct.unpack_from("<I", payload, 0x19250)[0] == 0x0200FD61
    cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
    cpu.mem_map(EWRAM, 0x40000)
    cpu.mem_map(IWRAM, 0x8000)
    cpu.mem_write(EWRAM, payload)
    registers = [UC_ARM_REG_R0, UC_ARM_REG_R1, UC_ARM_REG_R2]
    state = {}

    def write(address, value, width=4):
        cpu.mem_write(address, value.to_bytes(width, "little"))

    def boundary(machine, address, _size, _user):
        call = dict(target=address | 1, args=[machine.reg_read(UC_ARM_REG_R0)])
        state["calls"].append(call)
        ordinal = len(state["calls"])
        for mutation in state["case"].get("mutations", []):
            if mutation["call"] == ordinal:
                write(mutation["address"], mutation["value"], mutation["width"])
        if ordinal == state["case"].get("abortAtCall"):
            state["aborted"] = True
            cpu.emu_stop()
        else:
            machine.reg_write(UC_ARM_REG_PC, machine.reg_read(UC_ARM_REG_LR))

    for offset in (0x10B84, 0x11AD8, 0x12490):
        cpu.hook_add(UC_HOOK_CODE, boundary, begin=EWRAM + offset, end=EWRAM + offset)
    cases = specifications(players, songs)
    for case in cases:
        state.clear()
        state.update(case=case, calls=[], aborted=False)
        cpu.mem_write(IWRAM, pattern(case["seed"], 0x8000))
        cpu.mem_write(EWRAM + 0x30000, bytes(0x1000))
        for context, tracks, _ in players:
            write(context, songs[case.get("song", 11)][0] if case["same"] else HEADER if case.get("priorityGate") else 0)
            write(context + 4, case["status"])
            write(context + 8, case["capacity"], 1)
            write(context + 9, case.get("playerPriority", 2), 1)
            write(context + 0xB, case.get("priorityGate", 0), 1)
            write(context + 0x2C, tracks)
            write(context + 0x34, case["signature"])
            for index in range(16):
                write(tracks + index * 0x50, 0, 1)
                write(tracks + index * 0x50 + 0x20, 0)
        write(0x03007FF0, INFO)
        write(0x030071DC, case.get("clearTarget", 0x0200FD61))
        write(INFO, case.get("infoSignature", SIGNATURE))
        write(INFO + 0x20, case.get("previous", 0x0200FFD9))
        write(INFO + 0x24, 0x03007000)
        write(INFO + 0x2C, 0x02011AD9)
        cpu.mem_write(HEADER, bytes([case["headerCount"], 0, case["headerPriority"], case["headerReverb"]]))
        write(HEADER + 4, 0x0201949C)
        for index in range(16):
            write(HEADER + 8 + index * 4, 0x0201D66C + index * 4)
        tracks = players[0][1]
        write(tracks, case.get("firstFlag", 0), 1)
        if case["channels"]:
            write(tracks + 0x20, CHANNEL)
        for index, channel in enumerate(case["channels"]):
            at = CHANNEL + index * 0x40
            write(at, channel["active"], 1)
            write(at + 1, channel["kind"], 1)
            write(at + 0x2C, tracks)
            write(at + 0x34, at + 0x40 if index + 1 < len(case["channels"]) else 0)
        for register, value in zip(registers, case["args"]):
            cpu.reg_write(register, value)
        cpu.reg_write(UC_ARM_REG_SP, IWRAM + 0x7F00)
        cpu.reg_write(UC_ARM_REG_LR, RETURN | 1)
        cpu.emu_start(EWRAM + case["entry"] + 1, RETURN, count=100000)
        assert state["aborted"] or cpu.reg_read(UC_ARM_REG_PC) == RETURN, case["name"]
        case.update(calls=state["calls"], aborted=state["aborted"],
                    ramSha256=parity.sha256(cpu.mem_read(IWRAM, SNAPSHOT_LENGTH)),
                    scratchSha256=parity.sha256(cpu.mem_read(HEADER, 0x1000)))
    return dict(romSha256=parity.sha256(rom), payloadSha256=parity.sha256(payload),
                nativeInstructionParity=True, hardwareModel="synthetic-PSG-soundmode-dynamic-clear",
                renderedAudioValidated=False, sequencerValidated=False, gardenEndToEndValidated=False,
                snapshot=dict(base=IWRAM, length=SNAPSHOT_LENGTH), players=players,
                songs=songs, headers=headers,
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
                       "Native Garden audio vectors differ")
    print(json.dumps(dict(cases=len(data["cases"]), renderedAudioValidated=False)))
