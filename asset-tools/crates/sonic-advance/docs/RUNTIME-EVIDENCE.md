# Runtime evidence

The desktop executable and browser WASM adapter both call `runtime::NativeRun` in this crate. See the [shared validation receipt](NATIVE-VALIDATION.md) for the reference inputs, independent comparisons, local commands and first known divergence. [Port status](PORT-STATUS.md) distinguishes traced subsystems from unresolved behavior.

The CLI's `replay ROM EXISTING_LOCAL_DIR FRAMES [STAGE]` accepts 1–36,000 frames. It publishes a P6 RGB image, JSON trace and receipt only into an existing directory named `.local`. The RGB digest excludes the image header; the trace digest covers the serialized JSON bytes. A name collision leaves the existing files intact and rolls back any newly published siblings. The receipt is last; a crash can leave siblings without a receipt. Run `cargo test --release --test runtime -- --include-ignored` with the private ROM environment set.
