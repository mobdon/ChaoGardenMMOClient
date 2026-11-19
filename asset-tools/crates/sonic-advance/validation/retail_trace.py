"""Reproduce the ASOE Neo Green Hill movement reference. Prints only hashes."""
import argparse
import ctypes as ct
import hashlib
import os
from pathlib import Path
import struct
import tempfile


ROM_HASH = "afc29e3325baf68a1822e88b25fcb59c8bdc29b8cdc48fa21c56defbd60ca020"
CORE_HASH = "d775ecef6da1da5489ff2e0a37c13826b510c73711f89e78cb8d757c6f8c1426"
TRACE_HASH = "f3191a348b928b9fcf28f22314a091a750675941f28b35707dbb5e42338f8f34"
Ptr, Uint, Size = ct.c_void_p, ct.c_uint, ct.c_size_t


class GameInfo(ct.Structure):
    _fields_ = [("path", ct.c_char_p), ("data", Ptr), ("size", Size), ("meta", ct.c_char_p)]


class MemoryDescriptor(ct.Structure):
    _fields_ = [("flags", ct.c_uint64), ("ptr", Ptr), ("offset", Size), ("start", Size),
                ("select", Size), ("disconnect", Size), ("len", Size), ("addrspace", ct.c_char_p)]


class MemoryMap(ct.Structure):
    _fields_ = [("descriptors", ct.POINTER(MemoryDescriptor)), ("count", Uint)]


def reference(core_path, rom_path, scratch):
    with core_path.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != CORE_HASH:
            raise ValueError("expected fingerprinted Windows x64 mGBA libretro core")
    if rom_path.stat().st_size != 8 * 1024 * 1024:
        raise ValueError("expected 8 MiB ASOE ROM")
    rom = rom_path.read_bytes()
    if hashlib.sha256(rom).hexdigest() != ROM_HASH:
        raise ValueError("wrong ROM revision")
    core = ct.CDLL(str(core_path.resolve()))
    iwram = None
    directory = os.fsencode(scratch)
    buttons = set()
    callbacks = []

    def register(name, signature, function):
        callback = ct.CFUNCTYPE(*signature)(function)
        callbacks.append(callback)
        getattr(core, "retro_set_" + name)(callback)

    def environment(command, value):
        nonlocal iwram
        command &= 0xffff  # Libretro experimental flag
        if command in (9, 31):  # system/save directory, always empty on startup
            ct.cast(value, ct.POINTER(ct.c_char_p))[0] = directory
        elif command == 36:
            mapping = ct.cast(value, ct.POINTER(MemoryMap)).contents
            for desc in mapping.descriptors[:mapping.count]:
                if desc.start == 0x03000000 and desc.len == 32768 and desc.ptr:
                    iwram = desc.ptr + desc.offset
        elif command == 17:
            ct.cast(value, ct.POINTER(ct.c_bool))[0] = False
        elif command != 10:
            return False
        return True

    register("environment", (ct.c_bool, Uint, Ptr), environment)
    register("input_poll", (None,), lambda: None)
    register("input_state", (ct.c_int16, Uint, Uint, Uint, Uint),
             lambda port, device, index, key: int(port == 0 and key in buttons))
    register("video_refresh", (None, Ptr, Uint, Uint, Size), lambda *args: None)
    register("audio_sample", (None, ct.c_int16, ct.c_int16), lambda *args: None)
    register("audio_sample_batch", (Size, Ptr, Size), lambda data, frames: frames)
    rom_buffer = ct.create_string_buffer(rom)
    info = GameInfo(os.fsencode(rom_path), ct.cast(rom_buffer, Ptr), len(rom), None)
    core.retro_load_game.restype = ct.c_bool
    core.retro_init()
    loaded = False
    try:
        loaded = core.retro_load_game(ct.byref(info))
        if not loaded:
            raise RuntimeError("core rejected ASOE")
        digest = hashlib.sha256()
        for frame in range(2400):
            buttons.clear()
            if frame == 60 or frame == 61 or any(s <= frame < s + 3 for s in (1050,1150,1250)):
                buttons.add(3)  # START from a blank in-memory flash
            if any(s <= frame < s + 3 for s in (1350,1490)):
                buttons.add(8)  # A selects Sonic
            if frame >= 1830:
                buttons.add(7)  # RIGHT
            if frame >= 1800 and 100 <= (frame - 1800) % 180 < 120:
                buttons.add(8)
            core.retro_run()
            if frame >= 1800:
                if iwram is None:
                    raise RuntimeError("core did not expose IWRAM")
                actor = ct.string_at(iwram + 0x5a20, 40)
                x, y, vx, vy = struct.unpack_from("<iihh", actor)
                cx, cy = struct.unpack("<hh", ct.string_at(iwram + 0x59d0, 4))
                rings, = struct.unpack("<H", ct.string_at(iwram + 0x4fec, 2))
                digest.update(struct.pack("<8i", x, y, vx, vy, cx, cy, actor[39], rings))
        return digest.hexdigest()
    finally:
        if loaded:
            core.retro_unload_game()
        core.retro_deinit()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("core", type=Path)
    parser.add_argument("rom", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as directory:
        result = reference(args.core, args.rom, directory)
    print(result)
    if result != TRACE_HASH:
        raise SystemExit("600-frame retail trace mismatch")
