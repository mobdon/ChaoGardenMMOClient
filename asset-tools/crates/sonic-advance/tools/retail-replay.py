"""Sample the original title or a Sonic spindash from a cold boot. No save files or frame dumps.

Usage: python tools/retail-replay.py LOCAL_ASOE_ROM LOCAL_MGBA_LIBRETRO_DLL [--title|--menu|--menu-tiny]
The DLL and ROM are identified before any external code runs. Outputs only
selected numbers and pixel hashes. No graphical or memory buffers are written.
"""
import ctypes as ct
import hashlib
import json
import os
from pathlib import Path
import struct
import sys

ROM_HASH = "afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020"
CORE_HASH = "d775ecef6da1da5489ff2e0a37c13826b510c73711f89e78cb8d757c6f8c1426"
P, U, S = ct.c_void_p, ct.c_uint, ct.c_size_t


class MemoryRegion(ct.Structure):
    _fields_ = [("flags", ct.c_uint64), ("ptr", P), ("offset", S), ("start", S),
                ("select", S), ("disconnect", S), ("length", S), ("space", ct.c_char_p)]


class MemoryMap(ct.Structure):
    _fields_ = [("entries", ct.POINTER(MemoryRegion)), ("count", U)]


class Game(ct.Structure):
    _fields_ = [("path", ct.c_char_p), ("data", P), ("size", S), ("meta", ct.c_char_p)]


def verified(path, limit, digest):
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit or hashlib.sha256(data).hexdigest() != digest:
        raise ValueError("unrecognized source or reference emulator")
    return data


def replay(rom, dll, title=False, menu=None):
    data = verified(rom, 8388608, ROM_HASH)
    verified(dll, 32 * 1024 * 1024, CORE_HASH)
    core = ct.CDLL(str(dll.resolve()))
    mem = {}
    buttons = set()
    keep = []
    pixel_format = None
    video_frames = {}
    title_ticks = (0, 25, 49, 50, 62, 90, 122, 123, 154, 155, 187)
    # The first affine update (Rust tick 50) was observed at retail frame 1102.
    title_origin = 1052
    if menu:
        title = True
        title_origin = 1250
        title_ticks = (0, 1, 2, 3, 4, 9, 10, 11, 19, 20, 21, 39, 40, 41, 48, 49, 50, 57, 58, 59, 69, 70, 71, 75, 95, 117, 118, 119)
    frame = -1

    def bind(kind, signature, func):
        callback = ct.CFUNCTYPE(*signature)(func)
        keep.append(callback)
        getattr(core, "retro_set_" + kind)(callback)

    def environment(cmd, pointer):
        nonlocal pixel_format
        cmd &= 65535
        if cmd == 10:
            pixel_format = ct.cast(pointer, ct.POINTER(ct.c_int))[0]
        if cmd == 36:
            regions = ct.cast(pointer, ct.POINTER(MemoryMap)).contents
            for region in regions.entries[:regions.count]:
                if region.start == 0x03000000 and region.ptr and region.length >= 0x5a20 + 72:
                    mem["iwram"] = region.ptr + region.offset
        return cmd in (10, 36)

    def video(pointer, width, height, pitch):
        tick = frame - title_origin
        if not title or tick not in title_ticks or not pointer:
            return
        if pixel_format != 2 or (width, height) != (240, 160) or not 480 <= pitch <= 2048:
            return
        # libretro RGB565 uses a six-bit green channel. Compare the upper
        # five bits of each channel in GBA palette order (little endian).
        source = ct.string_at(pointer, pitch * height)
        packed = bytearray(width * height * 2)
        for y in range(height):
            row = source[y * pitch:y * pitch + width * 2]
            for x, (value,) in enumerate(struct.iter_unpack("<H", row)):
                color = (value >> 11) | (((value >> 6) & 31) << 5) | ((value & 31) << 10)
                struct.pack_into("<H", packed, 2 * (y * width + x), color)
        video_frames[tick] = hashlib.sha256(packed).hexdigest()

    bind("environment", (ct.c_bool, U, P), environment)
    bind("input_poll", (None,), lambda: None)
    bind("input_state", (ct.c_int16, U, U, U, U), lambda port, dev, idx, key: int(port == 0 and key in buttons))
    bind("video_refresh", (None, P, U, U, S), video)
    bind("audio_sample", (None, ct.c_int16, ct.c_int16), lambda *_: None)
    bind("audio_sample_batch", (S, P, S), lambda _, frames: frames)
    core.retro_init()
    raw = ct.create_string_buffer(data)
    game = Game(os.fsencode(rom), ct.cast(raw, P), len(data), None)
    core.retro_load_game.restype = ct.c_bool
    loaded = False
    rows = []
    try:
        loaded = core.retro_load_game(ct.byref(game))
        if not loaded:
            raise ValueError("emulator rejected local source")
        for frame in range(title_origin + max(title_ticks) + 1 if title else 1823):
            buttons.clear()
            if frame in (60, 61) or 1050 <= frame < 1053:
                buttons.add(3)  # Start: boot and title
            if menu:
                if frame == 1250: buttons.add(3)
                if frame == 1260: buttons.add(4)  # Up wraps to Tiny Garden
                if menu == "regular" and 1270 <= frame < 1315: buttons.add(5)
                if frame == 1320: buttons.add(8)
            elif not title:
                if 1150 <= frame < 1153 or 1250 <= frame < 1253: buttons.add(3)
                if 1350 <= frame < 1353 or 1490 <= frame < 1493: buttons.add(8)
                if 1800 <= frame <= 1808: buttons.add(5)
                if frame in (1801, 1803, 1805): buttons.add(8)
            core.retro_run()
            if not title and frame >= 1799:
                p = ct.string_at(mem["iwram"] + 0x5a20, 72)
                x, y, vx, vy, speed, _, height = struct.unpack_from("<iihhhbb", p)
                charge, = struct.unpack_from("<h", p, 0x16)
                anim, variant = struct.unpack_from("<HH", p, 0x42)
                rows.append([frame, x, y, vx, vy, speed, height, charge, p[64], anim, variant])
        if title:
            if len(video_frames) != len(title_ticks):
                raise ValueError("title video format or frame coverage differs")
            return [{"tick": t, "retailFrame": title_origin + t, "bgr555Sha256": video_frames[t]} for t in title_ticks]
        if rows[0][1:6] != [26624, 113920, 0, 0, 0]:
            raise ValueError("retail replay missed the settled Neo Green Hill start")
        return {"columns": ["frame", "xQ8", "yQ8", "vx", "vy", "groundSpeed", "halfHeight",
                            "chargeRegister", "charState", "animation", "variant"], "rows": rows}
    finally:
        if loaded:
            core.retro_unload_game()
        core.retro_deinit()


if __name__ == "__main__":
    if len(sys.argv) not in (3, 4) or (len(sys.argv) == 4 and sys.argv[3] not in ("--title","--menu","--menu-tiny")):
        sys.exit(__doc__)
    print(json.dumps(replay(Path(sys.argv[1]), Path(sys.argv[2]), len(sys.argv) == 4, {"--menu":"regular","--menu-tiny":"tiny"}.get(sys.argv[-1])), indent=2))
