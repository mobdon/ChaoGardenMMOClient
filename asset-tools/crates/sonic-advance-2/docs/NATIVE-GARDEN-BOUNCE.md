# Cartridge Chao Bounce

The A2NE embedded Tiny Chao Garden contains Chao Memory and **Chao Bounce**.
The Bounce callback graph begins at payload address 0x0200D294: it initializes
two rows of thirteen targets and creates the Chao, spring and stage-dependent
moving objects. Its original reverse-engineering symbols remain unnamed;
the game identification follows the established Tiny Garden source catalog,
not a newly invented meaning for an opaque table. RPS and Ball use different
supplied cartridges; Search is not established from this A2NE payload.

`native_garden_second` provides a cartridge-only practice boundary:

```rust,ignore
let mut game = BouncePractice::new(&verified_rom)?;
let view = game.tick(held_buttons)?; // exactly one native video frame
let result = game.finished();       // local (won, rings), never a save write
```

Buttons use the native A 1, B 2, Start 8, right 16, left 32, up 64 and down 128 masks.
The wrapper derives pressed edges from accepted held words. Unknown button
bits and a 36,000-frame practice budget fail closed; rejected input preserves
state. The public view is serializable and includes native actor positions,
tile/descriptor offsets, flags, sprite parts and affine inputs. A finished
practice round stays frozen. Input edge generation and the bounded local frame
transport are explicit host bindings, not an emulated hardware input driver.

## Reused Rust boundary

The complete existing user-local Rust `tiny-garden-core::native_bounce` task
engine is reused under `src/reused/native_bounce.rs`, together with its explicit
BIOS scalar boundary. No second arithmetic implementation or server dependency
was introduced. `catalog/native-garden-second-reuse.json` records the exact
source input hashes for the Rust engine, BIOS boundary, probe and verifier.
The engine input was 65,782 bytes, SHA-256
`920298ed25244a55f302b7453c69ff6e5196826d103b6e97101dad668b1ef795`.

Changes at this crate boundary replace unrelated shop validation with the same
exact payload identity gate, change local import paths, serialize the existing
view and expose the physical-input practice wrapper. Native task arithmetic is
retained. The separate server repository was read only; no source, runtime,
profile, socket or service was changed or started for this port.

The source gate requires the 191,340-byte decoded A2NE program, SHA-256
`84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca`,
SHA-1 `2c7da8e1c9af85986eecf4effae39197cdac9abf`. Source tables—including the
256-entry trig table at 0x1FB74, spring walk table at 0x2EB60 and five landing
offsets at 0x2EB64—are read from those exact bytes. Original art, tables, program
bytes and generated traces remain local.

The reused core covers Chao fixed-point movement, spring movement and timing,
launches, target collection/burst, four-stage progression, three lives, moving
objects and their collision responses, native spring child poses/affines,
pause/quit, losses, fade and return/result boundaries. It does not award rings
to any canonical MMO Chao or modify a supplied save.

## Actual Thumb comparison

`tests/native_garden_second_oracle.py` executes the private payload in
Unicorn 2.1.4 and compares the same native callback graph with this crate's Rust
core. Original task allocation and scheduling run in the Thumb reference.
Graphics BIOS copies/decompression and affine output are bounded service models;
sound callbacks are counted and skipped. Every traced scalar Sqrt/ArcTan2 input
is checked against the existing independently compiled numerical BIOS reference.
That reference is fingerprint-gated; this is not an authentic BIOS dump or a
cycle-accurate hardware claim.

The four-stage × eight-policy corpus compares 48,503 actual native frames,
including 366,675 spring-pose checks, 73,695 spring-affine checks, 37,776 moving
object affine checks and 6,347 independently checked BIOS vectors. The corpus
contains ordinary input-driven play/loss, pause/quit and pause/resume. One
separately identified mode empties targets to reach clear/next-stage branches;
that mode is controlled-state branch coverage, not an input-only victory.

A separate exact practice policy—one-frame A every 60 frames, no state writes—
matches the actual Thumb program through its complete 1,097-frame round. It
returns a native loss with 7 rings. The Rust test asserts that duration/result
and a deterministic public-view transcript fingerprint. No save or service is
used. This proves an executable full round, not a successful four-stage win.
JSON evidence is in `catalog/native-garden-bounce-parity.json`. The verifier digest records the Windows run; `verifierGitLfSha256` identifies the same Python source with Git line endings.

## Reproduction and limits

Run `cargo test --test source_native_garden_second -- --include-ignored` with
`A2NE_ROM` set to the verified local cartridge. Library tests also exercise
native task-slot reuse, flags, pause, invalid-state rollback and numerical BIOS
boundaries.

Run the Python verifier with `--payload <private decoded garden> --output-dir
<new ignored .local directory> --bios-reference <fingerprint-matching numerical
reference executable> --unicorn-dir <local Unicorn 2.1.4 package> --frames 2000`.
It compiles the probe against this standalone crate, offline. Add
`--practice-only` for the exact default-spawn, input-only complete round.

Native graphics/audio output, GBA cycles/PWM, an authentic BIOS machine-code
implementation, physical flash/transfer and native artwork display are outside
this task core. The view provides original descriptors and affine inputs for a
separate renderer; a hand-drawn replacement is not implied. No browser, Android,
MMO service or durable reward acceptance is claimed by this cartridge practice
port.
