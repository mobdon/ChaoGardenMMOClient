# Sonic Advance US audio evidence

The standalone Rust audio example executes MP2K track commands from a locally supplied ASOE revision 0 cartridge. It also exports the playable portion of an ordinary uncompressed Direct Sound sample as a one-shot mono WAV. It verifies the existing ASOE fingerprint before selecting a song. Track numbers start at one and voices start at zero.

~~~powershell
cargo run --example audio -- "$env:SONIC_ADVANCE_ASOE_ROM" 1 trace 1 882
cargo run --example audio -- "$env:SONIC_ADVANCE_ASOE_ROM" 1 sample 70 "$env:TEMP/voice70.wav"
cargo test --test music -- --include-ignored
~~~

Choose a new local output path for the WAV. The example refuses to overwrite existing files.

The reference is SAT-R/sa2 commit 2dd3f17d30cd3c156e508aac0017620ae485b9ba. Sequence opcodes and clock lengths come from sound/MPlayDef.s. The sequence dispatch, running status threshold, note parameter reuse, three-level pattern stack, repeat counter, and mixer channel sample count follow src/lib/m4a/m4a0.s. Sample encoding and frequency scaling follow tools/aif2pcm/main.c. The US cartridge table at offset 0x6D76AC was independently located from sound/sa1/song_table.inc and is recorded in the asset catalog. The trace follows the pointers in the supplied cartridge.

The installed-cartridge test executed all 574 tracks from all 180 nonempty song entries through FINE or score tick 4096. This checks decoding and termination over that interval. For an independent musical check, sound/sa1/songs/song001.s specifies nine W96 waits, a tempo write of 84, and four C-sharp notes at ticks 864, 870, 876, and 882 on track one. The Rust trace matches their keys, velocity, gate lengths and voice, including omitted running parameters.

Song 1 voice 70 resolves through the US voicegroup to wave offset 0x6D8050. An independent Python struct read gave 10,764,288 frequency units, no loop, and 5,312 playback bytes. Python’s wave reader reopened the CLI output at 10,512 Hz with 5,312 mono 8-bit frames. Reversing the WAV signedness conversion reproduced SHA-256 0f364f8aaf3c13e50256a7c715c60a3ba3e4009f69ef9a8979a1963f85789a29. Only the hash and source metadata are tracked.

The trace uses sequence ticks and reports raw control writes. Full song mixing, channel arbitration, PSG instruments, key splits, percussion, envelopes, modulation, tempo-to-VBlank scheduling, looped sample playback, and connection to stage audio remain open. Unsupported commands and invalid data produce errors. The interpreter caps a request at 100,000 instructions and rejects pattern nesting beyond the native three levels.
