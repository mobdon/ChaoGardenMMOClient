"""Compare actual-ROM Rust sequence fingerprints to pinned source MIDI.

MIDI/native audio inputs and optional WAV outputs remain ignored/local. Public
evidence contains identities/counts/hashes only, never notes or sample bytes.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct

COMMIT = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
MIDIS = {
    13: ("mus_leaf_forest__act_1.mid", "af56c890c615e915d1509053c8cfcde8f1212c75"),
    14: ("mus_leaf_forest__act_2.mid", "c00311f08e7bd61fac4c3ed8536c6d8b5ac584e1"),
    15: ("mus_hot_crater__act_1.mid", "80868a2357ce66b0c1f8373c2a7f1d445998db15"),
    16: ("mus_hot_crater__act_2.mid", "f2d7a3a23b4ce0b2eb0f736c2f7a8055fb77d995"),
    17: ("mus_music_plant__act_1.mid", "2ff6d2faf7047dcd234bf91b56606f2bf9b8b9ae"),
    18: ("mus_music_plant__act_2.mid", "f21d560d6151d192c3ecd109bcaac71bf8a2af77"),
    19: ("mus_ice_paradise__act_1.mid", "3cd49e2dad74b57746aabf152c8c7f301c66b99b"),
    20: ("mus_ice_paradise__act_2.mid", "843a241a337d837d9d581305d3576611ced4da80"),
    21: ("mus_sky_canyon__act_1.mid", "1793b9883483542521fa5fbe2074e6eb7ab69c38"),
    22: ("mus_sky_canyon__act_2.mid", "6a90fd8ec3859cdc08120761b40ac053ce6bc4b2"),
    23: ("mus_techno_base__act_1.mid", "022b2d3998bcde064120baaeab3bf15518485665"),
    24: ("mus_techno_base__act_2.mid", "17680e310ff480b62cd89adb1bd900a5c1aa3273"),
    25: ("mus_egg_utopia__act_1.mid", "3820476caa438ee69b1d6dc682a4073021bf8243"),
    26: ("mus_egg_utopia__act_2.mid", "5871665f049e3b365497fc8b6722359523a69573"),
    102: ("se_ring.mid", "441990298f231c354777f5ba7df7cc599e04de3e"),
    112: ("se_sonic_midair_somersault.mid", "d6c45f517e8cb59ec7c42e908fa5e4396f6f8eff"),
    113: ("se_sonic_insta_shield.mid", "3ade524f6033825f4a43b09fcb04ab153204d812"),
    115: ("se_brake.mid", "70e201090fa12a4caaeeaf5c51fef7824528aa66"),
    116: ("se_jump.mid", "95bfb60185a60dec6dc60b74fca2d9962da64364"),
    117: ("se_ring_copy.mid", "441990298f231c354777f5ba7df7cc599e04de3e"),
    118: ("se_rings_lost.mid", "8c83b643c06c8f1efbb5b5c7a5f9fb5c17a273fa"),
    119: ("se_life_lost.mid", "05242b1c0fe086c1d31afd3bea25d66b30356348"),
    138: ("se_itembox.mid", "3ee0a8de959c87387a50cde4de0fbbd298abc28f"),
    139: ("se_timer.mid", "24504270f30b7ca8e13842424ea59294118e76c2"),
    140: ("se_stage_result_counter.mid", "75b1c642691a6f53a353f9779531160bb6a94211"),
    141: ("se_stage_result_counter_done.mid", "a13a626e3c7a6195e002df2b2706ce8dcf475c68"),
    149: ("se_time_up.mid", "374cc318912617f1d2122ccc4362eba84bdabbd2"),
    152: ("se_checkpoint.mid", "132eda40bb74719265b562df3a721ee3bcd1e22d"),
    153: ("se_spring.mid", "6c5fd74de746b54d011c3d19d0348c61739c9924"),
    157: ("se_157.mid", "4dfca4ef3cfdf1e3b2159799fb2d1a307323fc39"),
    221: ("se_221.mid", "60fc015209ce6dfec4371a72330ae0a7588c7153"),
    232: ("se_long_brake.mid", "e95e2f8987fa33a5a928aa8f4eb4dd3c8bf09980"),
    401: ("mus_act_clear.mid", "a5a0c3ae101b3ae19a4e499155b0eb3195f8a53d"),
    407: ("mus_1_up.mid", "127b115fa52876d0d6add2036d0a3a8a61a3543e"),
}


def midi_notes(raw):
    if len(raw) > 1_048_576 or raw[:4] != b"MThd" or struct.unpack_from(">I", raw, 4)[0] != 6:
        raise ValueError("Invalid/oversized source MIDI header")
    fmt, count, division = struct.unpack_from(">HHH", raw, 8)
    if fmt != 1 or division != 24 or not 1 <= count <= 32:
        raise ValueError("Unexpected source MIDI format/clock")
    all_tracks, offset = [], 14
    for _ in range(count):
        if raw[offset:offset + 4] != b"MTrk":
            raise ValueError("Missing source MIDI track")
        length = struct.unpack_from(">I", raw, offset + 4)[0]
        data = raw[offset + 8:offset + 8 + length]
        if len(data) != length:
            raise ValueError("Truncated source MIDI track")
        offset += 8 + length
        pc, tick, status = 0, 0, 0
        program, active, notes = [0] * 16, {}, []

        def vlq():
            nonlocal pc
            value = 0
            for _ in range(4):
                b = data[pc]
                pc += 1
                value = value * 128 + (b & 127)
                if b < 128:
                    return value
            raise ValueError("Unbounded source MIDI VLQ")

        for _ in range(100_000):
            if pc == len(data):
                break
            tick += vlq()
            if data[pc] >= 128:
                status = data[pc]
                pc += 1
            if status == 255:
                pc += 1
                length = vlq()
                pc += length
                continue
            if status in (240, 247):
                length = vlq()
                pc += length
                continue
            kind, channel = status >> 4, status & 15
            if not 8 <= kind <= 14:
                raise ValueError("Unknown source MIDI status")
            a = data[pc]
            pc += 1
            b = 0
            if kind not in (12, 13):
                b = data[pc]
                pc += 1
            if kind == 12:
                program[channel] = a
            elif kind == 9 and b != 0:
                index = len(notes)
                notes.append([tick, a, b, None, program[channel]])
                active.setdefault((channel, a), []).append(index)
            elif kind == 8 or kind == 9 and b == 0:
                queue = active.get((channel, a), [])
                if not queue:
                    raise ValueError("Source MIDI ends an inactive note")
                note = notes[queue.pop(0)]
                note[3] = tick - note[0]
        else:
            raise ValueError("Source MIDI event budget exceeded")
        if notes:
            if any(note[3] is None for note in notes):
                raise ValueError("Unclosed source MIDI note")
            all_tracks.append(notes)
    if offset != len(raw):
        raise ValueError("Trailing source MIDI data")
    return all_tracks


def main():
    parser = argparse.ArgumentParser()
    for key in ["references", "rust-log", "rom", "report"]:
        parser.add_argument("--" + key, type=Path, required=True)
    args = parser.parse_args()
    rom = args.rom.read_bytes()
    if hashlib.sha256(rom).hexdigest() != "f2b7a3329401a02706d87560f61da3e6da2ca42f2b9ae98a86d78e0529cb7260":
        raise ValueError("Not the verified A2NE ROM")
    actual = [json.loads(line[6:]) for line in args.rust_log.read_text(encoding="utf-8-sig").splitlines() if line.startswith("AUDIO ")]
    if [row["id"] for row in actual] != list(MIDIS):
        raise ValueError("Missing or reordered Rust cue evidence")
    verified = []
    for row in actual:
        filename, expected_blob = MIDIS[row["id"]]
        raw = (args.references / filename).read_bytes()
        blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        if blob != expected_blob:
            raise ValueError("Pinned MIDI Git blob mismatch: " + filename)
        tracks = midi_notes(raw)
        selected = [[note for note in track if note[0] <= row["scoreThrough"]] for track in tracks]
        output = b"".join(struct.pack("<I", len(track)) + b"".join(struct.pack("<IIIII", *note) for note in track) for track in selected)
        if len(tracks) != row["tracks"] or sum(map(len, selected)) != row["scoreNotes"] or hashlib.sha256(output).hexdigest() != row["scoreSha256"]:
            raise ValueError("Original MIDI/actual-ROM sequence disagreement: " + filename + " expected " + hashlib.sha256(output).hexdigest() + " got " + row["scoreSha256"])
        sample = row["pcmSource"]
        if sample:
            header = struct.unpack_from("<IIII", rom, sample["waveOffset"])
            if header != (0, sample["pitch1024"], 0, sample["length"]):
                raise ValueError("Actual PCM header differs")
            if hashlib.sha256(rom[sample["dataOffset"]:sample["dataOffset"] + sample["length"]]).hexdigest() != sample["sha256"]:
                raise ValueError("Actual PCM source bytes differ")
        verified.append({**row, "midi": {"file": "sound/sa2/songs/midi/" + filename, "git_blob_sha1": blob, "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)}})
    report = {"reference_commit": COMMIT,
              "rom_sha256": hashlib.sha256(rom).hexdigest(),
              "song_table_offset": 0xAD4F4C, "song_table_rows": 507,
              "sample_rate": 10512, "samples_per_driver_frame": 176,
              "scope": "Original MIDI note tick/key/velocity/gate/voice fingerprints agree with actual-ROM Rust MP2K traces for all 34 cues; complete stage introduction/first loop, SFX/jingles through completion. Actual ring PCM bytes/header verified. PCM output hashes are deterministic preview evidence, not independent retail waveform parity.",
              "cues": verified}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"cues": len(verified), "notes": sum(row["scoreNotes"] for row in verified), "pcm_bytes": sum(row["pcmBytes"] for row in verified)}))


if __name__ == "__main__":
    main()
