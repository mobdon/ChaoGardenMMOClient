# Run the Sonic Advance 2 Rust course runner

The local runner launches Sonic in either normal act of the seven course zones. Rust reads an owner-supplied A2NE cartridge, checks both the cartridge and embedded garden hashes, and executes player, camera, terrain and object code. The loopback browser sends physical buttons and presents a 240×160 frame. This runner is separate from the MMO and from the cartridge's Tiny Garden.

```powershell
$crate = 'asset-tools/crates/sonic-advance-2/Cargo.toml'
$rom = (Resolve-Path '.local/gba/A2NE-f2b7a3329401.gba').Path
cargo run --locked --release --manifest-path $crate --bin a2ne-native -- stages $rom
cargo run --locked --release --manifest-path $crate --bin a2ne-native -- serve $rom 0 0
cargo run --locked --release --manifest-path $crate --bin a2ne-native -- replay $rom asset-tools/crates/sonic-advance-2/examples/leaf-forest-inputs.json 0
```

The last number selects the level: 0/1 for Leaf Forest, 4/5 for Hot Crater and so on through 24/25. The stage command lists the 31 terrain slots; boss and unused slots are inspectable but cannot be launched as normal acts. The optional port defaults to 42872; 0 selects a free loopback port. Select Run, then press left/right or Z/Space for A. Reset returns to the selected act. Each process owns one in-memory session. A replay accepts at most 4,096 segments, 36,000 frames and 256 KiB of JSON.

Movement includes native ground and air controls, variable-height jumps, braking, turning, ordinary springs and no-target Sonic air actions. The native camera runs before object admission. Rings can be collected once; their sound requests alternate pan. The stage HUD runs before the player; the stage clock runs after the player. Layer triggers, checkpoint crossing, pit deaths, life loss, timeout and memory-only stage recreation have explicit native rules. A retry uses the recorded checkpoint position and time. The runner bypasses the countdown on start and retry. Frame zero is a primed host preview.

Foreground placement, animated tiles, affine walking sprites, ring/pickup art and HUD graphics come from the cartridge. Leaf Forest also renders the static and scrolling bands of its two native backgrounds. Other zones currently render original foregrounds over their backdrop color. This host does not reproduce scanline DMA, complete OAM limits, all zone backgrounds or the entire task graph. Enable Sound effects for the supported native cue previews. Preview stage music plays the original introduction and one complete sequence loop for the selected act. Audio previews use original instrument data; the browser plays decoded samples through Web Audio. The mixer and simultaneous voice behavior are approximate. See [audio evidence](NATIVE-AUDIO.md).

Mon and Buzzer update after the player: Sonic can home in, take damage, lose rings, recollect them, defeat an enemy, and earn score. Scattered rings share the native 32-slot pool. A visible lever and the actual finish toggle are separate native entities. On an ordinary finish the clock stops, victory and results sprites run, bonuses count down (A can skip), and the native journal receives the updated unlock. Act 1 starts act 2; act 2 reports the boss destination and stops until its encounter is implemented. Reset starts a fresh local session. The host never writes to the source ROM or a real save.

The 227-frame Leaf Forest replay collects three rings, jumps, lands, brakes and turns. Longer right-input checks run 1,800 frames in each of the 14 acts and include layer changes, spring launches and death/retry. They neither reach all finishes nor certify the campaign. [Validation](NATIVE-VALIDATION.md) separates these integrated regressions from independent source comparisons. [Coverage](NATIVE-PORT.md#full-native-game-coverage-boundary) lists the missing retail systems.
