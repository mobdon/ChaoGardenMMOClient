# PC story sequence reader and offline cursor

`story::read(exe, config)` verifies the original Steam executable and resolves the `type=storysequence` mappings in SA Tools 786's PC_SA2 `ADVERTISE.ini`. The result is keyed by the literal section names. Each sequence holds the mapped RVA and configured source filename, ordered entries, and the complete native terminator. Each twelve-byte little-endian entry contains a signed byte type and character, a signed 16-bit level, and four signed 16-bit event words. Type 0 requests events, 1 requests a level, 2 terminates the array, and 3 presents credits. Set selection, cutscene data, executable code, inventory and save progression are separate.

The first `-1` terminates the active event list. The record still retains any following words. The verified Last Story record at RVA `0x133A5B0` contains `[206,207,-1,0]`. Only events 206 and 207 are active. Event 0 is a real event when it occurs before a sentinel. A full four-event record needs no sentinel. A type-2 row retains its ten payload bytes and never dispatches an action. The decoder rejects unknown entry types, unsupported character/level IDs, negative active event IDs other than the sentinel, and event/non-event profiles that would require guessing. Mapping and read budgets bound unterminated input. Positive event numbers are source identifiers; their files and playback are outside this reader.

`Cursor::new(&sequence)` starts an offline consumer at the first entry. `dispatch()` returns the current event, level request or credits and a token scoped to that cursor, entry and event slot. Polling again returns the same outstanding dispatch. `acknowledge(&token)` advances exactly once. A stale, foreign or unsolicited token leaves the cursor unchanged. Multi-event entries retain their authored order, including Last Story's opening 200, 201, 602 and 203. The cursor reaches `finished()` only at the native terminator. For Hero Story, the authored event 429 follows the credits. Acknowledgment is local flow control. It grants no mission clear, unlock or durable save, and it performs no cinematic or gameplay callback.

## Independent comparison

The 15 original PC sequence mappings comprise 143 nonterminal records and 15 terminators. Expanding the event slots yields 171 offline dispatches. `tests/story_local.rs` compares the values of every active event, character, level and type with a SHA-256 obtained through the independently executable SA Tools splitter. It also checks every terminator, physical row RVA, and the complete ordered dispatch/acknowledgment cycle of all sequences. Unrecognized inputs and stale tokens are exercised with standalone tests. The original post-sentinel word and the postcredits entry are covered explicitly.

The PowerShell 7 oracle `tests/story-reference.ps1` loads the exported INI for each configured filename through `SplitTools.SA2StoryList.Load(string)`, then hashes its returned values in ordinal section order. The verified SplitTools DLL digest is `5da8beb7288f1d7562fafd34c3b03a098d6ce177a8691ec98ae0df5b8254723f`. The independent semantic digest is `4f362db04ae1dd46801d9db9044f6e825e6d5783b31c28e554d457630548c7ed`. The full PC ADVERTISE.ini digest is `786f9083923d5f2dbec73fdb0be284154ced16c433b335386bba0af89272a3a0`. The reader uses the same verified executable revision as the native PC object-table reader.

With `SA2_GAME_PATH` pointing to the game and `SA2_PC_CONFIG` to PC_SA2, run from the crate:

```powershell
cargo test --locked --lib story::
cargo test --locked --test story_local -- --ignored --nocapture
./tests/story-reference.ps1 -SplitTools <SplitTools.dll> -StoryMappings <ref.ini> -SplitDirectory <split-output>
```

The split outputs and game executable stay local. This slice verifies authored sequence data and a consumer protocol; actual event playback, mission results, branches controlled elsewhere in the game, and persistent campaign progress remain unverified.
