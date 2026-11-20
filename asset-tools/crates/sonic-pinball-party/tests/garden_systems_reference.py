#!/usr/bin/env python3
"""Authenticate A3VE and reproduce Garden system-map metadata only.

This is source-span evidence, not native instruction or end-to-end parity.
No ROM, decoded program, image, save or audio bytes are emitted. The optional
write destination is restricted to this script's metadata fixture.
"""

import argparse
import json
import os
from pathlib import Path
import struct

import reference_parity as parity
import spp_vectors  # noqa: E402


FIXTURE = spp_vectors.path("garden-systems-vectors.json")
PAYLOAD_SHA = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb"
PAYLOAD_LENGTH = 0x2E12C
STORED_SHA = "72fd1b2645fc1358ad131a697dac1c53b4ae57f195759ff78e1535ebda70d29f"

# End-exclusive inspection spans. Named action groups can contain several
# handlers, branch tables and literal pools; they are not Rust coverage claims.
CODE = {
    "armResetAndIrq": (0, 0xC4),
    "initialization": (0xC4, 0x168),
    "mainLoop": (0x168, 0x19C),
    "saveLoader": (0x248, 0x3AC),
    "saveWriter": (0x3AC, 0x43C),
    "saveInitializer": (0x43C, 0x480),
    "joybusInitialize": (0x480, 0x590),
    "joybusMaintenance": (0x590, 0x6B8),
    "joybusIrqGroup": (0x6B8, 0xBD0),
    "input": (0xBD0, 0xC6C),
    "inputInitialize": (0xC6C, 0xC90),
    "taskInitialize": (0xC90, 0xD04),
    "taskAllocate": (0xD04, 0xDB0),
    "taskDispatch": (0xEAC, 0xED4),
    "oamComposition": (0x1088, 0x117C),
    "oamUpload": (0x12E0, 0x12F8),
    "queuedBgTransfer": (0x1420, 0x151C),
    "gardenInitialize": (0x18C8, 0x1B58),
    "gardenStaticLoader": (0x1C60, 0x1D68),
    "gardenFrame": (0x1D68, 0x1E00),
    "pauseAndSaveUi": (0x218C, 0x2364),
    "ringsAndStatsHud": (0x292C, 0x2D94),
    "communicationTaskGroup": (0x2EB8, 0x3424),
    "objectSpawner": (0x3424, 0x3534),
    "chaoInitialize": (0x41E0, 0x4418),
    "chaoFrameAndActions": (0x4418, 0x5E58),
    "highestSongCallWitness": (0x4BC6, 0x4C06),
    "chaoMovement": (0x5E58, 0x5EFC),
    "chaoStateSelection": (0x5EFC, 0x6570),
    "shopInitialize": (0x7500, 0x7688),
    "shopPanel": (0x7688, 0x7978),
    "purchaseCategory0": (0x7978, 0x7B00),
    "purchaseCategory1": (0x7B00, 0x7C4C),
    "purchaseCategory2": (0x7C4C, 0x7DA0),
    "foodInteraction": (0x7DA0, 0x800C),
    "otherItemInteractions": (0x800C, 0x8324),
    "inventoryItemInitialize": (0x8324, 0x83E4),
    "soundInitialize": (0x105E8, 0x10660),
    "soundMain": (0x10660, 0x1066C),
    "songStart": (0x1066C, 0x10698),
    "songStartOrRestart": (0x10698, 0x106E4),
}
DATA = {
    "chaoStatePointers": (0x45D4, 0x46D4),
    "foodEffects": (0x1269C, 0x126CD),
    "shopCategoryOrder": (0x126CD, 0x126D4),
    "shopPrices0": (0x126D4, 0x126F0),
    "shopPrices1": (0x126F0, 0x12720),
    "shopPrices2": (0x12720, 0x1272C),
    "soundPlayers": (0x19BE0, 0x19C10),
    "soundSongRecords": (0x19C10, 0x19D38),
    "ballMap0": (0x2DD64, 0x2DDA0),
    "ballMap1": (0x2DDA0, 0x2DDB8),
    "ballMap2": (0x2DDB8, 0x2DDEE),
    "ballMap3": (0x2DDEE, 0x2DE1E),
    "ballMap4": (0x2DE1E, 0x2DE6E),
    "ballMap5": (0x2DE6E, 0x2DFAE),
    "ballRegisters": (0x2E074, 0x2E0B4),
    "ballBgPalette": (0x17074, 0x17214),
    "ballObjPalette": (0x1606C, 0x1614C),
}
COMPRESSED = {"ballBgTiles": 0x17214, "ballObjTiles": 0x1614C}


def source_spans(payload, specifications):
    return {
        name: {
            "space": "garden",
            "offset": start,
            "length": end - start,
            "sha256": parity.sha256(parity.span(payload, start, end - start)),
        }
        for name, (start, end) in specifications.items()
    }


def audio_metadata(payload):
    """Check bounded header/pointer structure, without decoding track bytecode."""
    headers, tracks, voice_groups = set(), set(), set()
    nonempty, counts = set(), []
    for index in range(37):
        pointer, player, _ = struct.unpack("<IHH", parity.span(payload, 0x19C10 + index * 8, 8))
        parity.require(player < 4, "song player outside four initialized records")
        offset = pointer - 0x02000000
        header = parity.span(payload, offset, 8)
        count = header[0]
        parity.require(count <= 16, "header exceeds native player capacity")
        headers.add(offset)
        counts.append(count)
        if count:
            nonempty.add(offset)
            voice_offset = struct.unpack_from("<I", header, 4)[0] - 0x02000000
            parity.span(payload, voice_offset, 12)
            voice_groups.add(voice_offset)
        track_words = parity.span(payload, offset + 8, count * 4)
        for at in range(0, len(track_words), 4):
            track_offset = struct.unpack_from("<I", track_words, at)[0] - 0x02000000
            parity.span(payload, track_offset, 1)
            tracks.add(track_offset)
    parity.require(min(headers) == 0x19D38, "song table/header boundary changed")
    return {
        "songRecordCount": 37,
        "uniqueHeaderCount": len(headers),
        "uniqueNonemptyHeaderCount": len(nonempty),
        "uniqueTrackPointerCount": len(tracks),
        "maximumHeaderTrackCount": max(counts),
        "voiceGroupOffsets": sorted(voice_groups),
        "highestObservedSongIndex": 36,
        "highestIndexLoad": 0x4C00,
        "highestIndexCall": 0x4C02,
        "nativeLookupHasExplicitCountBound": False,
        "trackVoiceSampleClosureValidated": False,
        "renderedAudioParity": False,
    }


def generate(path):
    rom = parity.bounded_read(Path(path), parity.ROM_LENGTH)
    parity.validate_rom(rom)
    payload, consumed = parity.decode(rom[parity.GARDEN_OFFSET:], PAYLOAD_LENGTH)
    parity.require(consumed == parity.GARDEN_STORED, "Garden stored length mismatch")
    stored = parity.span(rom, parity.GARDEN_OFFSET, consumed)
    parity.require(parity.sha256(stored) == STORED_SHA, "Garden stored digest mismatch")
    parity.require(len(payload) == PAYLOAD_LENGTH, "Garden decoded length mismatch")
    parity.require(parity.sha256(payload) == PAYLOAD_SHA, "Garden decoded digest mismatch")
    compressed = {}
    for name, offset in COMPRESSED.items():
        decoded, used = parity.decode(payload[offset:], maximum=PAYLOAD_LENGTH)
        compressed[name] = {
            "space": "garden",
            "offset": offset,
            "storedLength": used,
            "storedSha256": parity.sha256(parity.span(payload, offset, used)),
            "decodedLength": len(decoded),
            "decodedSha256": parity.sha256(decoded),
        }
    return {
        "schemaVersion": 1,
        "evidenceOnly": True,
        "verification": "authenticated-source-span-metadata-only",
        "nativeInstructionParity": False,
        "gardenEndToEndValidated": False,
        "romSha256": parity.sha256(rom),
        "payloadSha256": PAYLOAD_SHA,
        "payloadLength": PAYLOAD_LENGTH,
        "storedGarden": {
            "space": "rom",
            "offset": parity.GARDEN_OFFSET,
            "length": consumed,
            "sha256": STORED_SHA,
        },
        "codeSpans": source_spans(payload, CODE),
        "dataSpans": source_spans(payload, DATA),
        "compressedResources": compressed,
        "audioStructure": audio_metadata(payload),
        "evidence": {
            "mainEntry": 0x168,
            "mainLoop": 0x178,
            "mainCallbackSlot": 0x03004250,
            "inputBase": 0x03005310,
            "taskBase": 0x03005320,
            "taskStride": 0x24,
            "freeTaskCount": 70,
            "gardenInitializer": 0x18C8,
            "gardenCallback": 0x1D68,
            "chaoInitializer": 0x41E0,
            "chaoFrame": 0x4418,
            "chaoStatePointerCount": 64,
            "saveBase": 0x03004AA0,
            "workingRings": 0x03006194,
            "saveRingField": 0xC,
            "transferRingField": 0x64,
            "receiveBuffer": 0x03004270,
            "transferSaveCall": 0x3122,
            "shopPriceCounts": [7, 12, 3],
            "inventorySlots": 8,
            "foodEffectRows": 7,
            "foodEffectRowBytes": 7,
            "gardenTaskEntries": [
                0x3424, 0x35CC, 0x74C0, 0x41E0, 0x89B0, 0x83E4,
                0x8324, 0x3E04, 0x7500, 0x8A04, 0x218C, 0x89D4, 0x88B0,
            ],
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", default=os.environ.get("SPP_ROM"))
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    if not args.rom:
        parser.error("provide --rom or SPP_ROM")
    data = generate(args.rom)
    if args.write:
        parity.require(args.write.resolve() == FIXTURE.resolve(), "write only the metadata fixture")
        FIXTURE.write_text(json.dumps(data, indent=2) + "\n", encoding="utf8")
    else:
        expected = json.loads(parity.bounded_read(FIXTURE, 128 * 1024))
        parity.require(data == expected, "Garden system-map metadata differs")
    print(json.dumps({
        "codeSpans": len(data["codeSpans"]),
        "dataSpans": len(data["dataSpans"]),
        "compressedResources": len(data["compressedResources"]),
        "verification": data["verification"],
        "gardenEndToEndValidated": False,
    }))


if __name__ == "__main__":
    main()
