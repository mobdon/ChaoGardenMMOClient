//! Source-level A3VE palette engine, traced from this ROM independently.
//!
//! Native state at 0x02001c50 contains source/white/black pointers, 32 cached
//! coefficient bytes at +0x0c, and 32 requested coefficients at +0x2c. ROM
//! 0x5784 merely sets coefficients; ROM 0x5668 performs changed-bank work using
//! the actual packed BGR555 blend at 0x19d0. This is not a generic fade curve.
//!
//! Register arguments truncate to their native u8/u16 widths before bounds
//! checks. Native requests that would corrupt adjacent RAM are rejected rather
//! than being treated as additional valid palette banks. No original bytes or
//! ROM execution are included in this Rust runtime.

use std::fmt;

pub const COLOR_COUNT: usize = 512;
pub const BANK_COUNT: usize = 32;
pub const COLORS_PER_BANK: usize = 16;
pub const NATIVE_BLEND_OFFSET: usize = 0x19d0;
pub const NATIVE_PROCESS_OFFSET: usize = 0x5668;
pub const NATIVE_COPY_OFFSET: usize = 0x5724;
pub const NATIVE_SET_EFFECTS_OFFSET: usize = 0x5784;
pub const NATIVE_BLEND_SHA256: &str =
    "6a1a9c688b00cc706bc18c16f0dac34b09e8036a5cc6067c1ce0533ab5dbc062";
pub const NATIVE_PROCESS_SHA256: &str =
    "acc2b05c317cc2153f9fbf3816595391f1dd90d173621607db89abb4986f4e53";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteError {
    pub buffer: &'static str,
    pub start: usize,
    pub count: usize,
    pub available: usize,
}

impl fmt::Display for PaletteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "palette {} range {}+{} exceeds {}",
            self.buffer, self.start, self.count, self.available
        )
    }
}

impl std::error::Error for PaletteError {}

fn check_range(
    buffer: &'static str,
    start: usize,
    count: usize,
    available: usize,
) -> Result<(), PaletteError> {
    // Both original count loops skip all memory accesses when the truncated
    // count is zero, even if the unused starting register lies out of range.
    if count == 0 || (start <= available && count <= available - start) {
        return Ok(());
    }
    Err(PaletteError {
        buffer,
        start,
        count,
        available,
    })
}

/// Original 0x19d0 scalar operation: reference + floor((source-reference)*f/32)
/// on each packed color field, with field masking after the arithmetic shift.
/// Factor is a native u16. Factors >=32 take the raw halfword-copy path, which
/// preserves bit 15; interpolation clears bit 15. There is no saturation step.
pub fn blend_native_color(source: u16, reference: u16, factor_register: u32) -> u16 {
    let factor = factor_register as u16;
    if factor >= 32 {
        return source;
    }
    let mut result = 0;
    for mask in [0x001f_i32, 0x03e0, 0x7c00] {
        let source_field = i32::from(source) & mask;
        let reference_field = i32::from(reference) & mask;
        let product = (source_field - reference_field) * i32::from(factor);
        result |= ((reference_field + (product >> 5)) & mask) as u16;
    }
    result
}

/// Bounded 0x19d0 buffer operation. Source/reference pointers already point to
/// their first requested color; r2 is the absolute destination color index and
/// r3 the color count. All three scalar arguments retain native u16 truncation.
/// The >=32 CpuSet branch never reads the reference pointer.
pub fn blend_into_native(
    source: &[u16],
    reference: &[u16],
    destination: &mut [u16],
    destination_register: u32,
    count_register: u32,
    factor_register: u32,
) -> Result<usize, PaletteError> {
    let start = usize::from(destination_register as u16);
    let count = usize::from(count_register as u16);
    check_range("source", 0, count, source.len())?;
    check_range("destination", start, count, destination.len())?;
    if (factor_register as u16) < 32 {
        check_range("reference", 0, count, reference.len())?;
    }
    if count == 0 {
        return Ok(0);
    }
    // The interpolation loop walks backwards. Rust slices make source and
    // destination non-aliasing; invalid native pointer overlaps are not guessed.
    for index in (0..count).rev() {
        let base = if factor_register as u16 >= 32 {
            0
        } else {
            reference[index]
        };
        destination[start + index] = blend_native_color(source[index], base, factor_register);
    }
    Ok(count)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteState {
    source: [u16; COLOR_COUNT],
    output: [u16; COLOR_COUNT],
    black_reference: [u16; COLOR_COUNT],
    white_reference: [u16; COLOR_COUNT],
    previous: [u8; BANK_COUNT],
    effects: [u8; BANK_COUNT],
}

impl PaletteState {
    /// Mirrors initialization at 0x5618: cached coefficients ff, requested
    /// coefficients 00. The existing output buffer is preserved. ROM 0x67120
    /// and 0x67520 independently verify as 512 black/white halfwords each.
    pub fn new(source: [u16; COLOR_COUNT], output: [u16; COLOR_COUNT]) -> Self {
        Self {
            source,
            output,
            black_reference: [0; COLOR_COUNT],
            white_reference: [0x7fff; COLOR_COUNT],
            previous: [0xff; BANK_COUNT],
            effects: [0; BANK_COUNT],
        }
    }

    pub fn source(&self) -> &[u16; COLOR_COUNT] {
        &self.source
    }

    pub fn output(&self) -> &[u16; COLOR_COUNT] {
        &self.output
    }

    pub fn effects(&self) -> &[u8; BANK_COUNT] {
        &self.effects
    }

    pub fn previous(&self) -> &[u8; BANK_COUNT] {
        &self.previous
    }

    /// Mirrors 0x5714's pointer replacement without inventing cache invalidation.
    /// Its r1 updates the black reference and r2 updates the white reference.
    pub fn configure_sources(
        &mut self,
        source: [u16; COLOR_COUNT],
        black_reference: [u16; COLOR_COUNT],
        white_reference: [u16; COLOR_COUNT],
    ) {
        self.source = source;
        self.black_reference = black_reference;
        self.white_reference = white_reference;
    }

    /// ROM 0x5784. No colors are changed here. u8 register truncation includes
    /// count 256 becoming zero. Requests outside 32 real banks fail atomically.
    pub fn set_effects(
        &mut self,
        start_register: u32,
        count_register: u32,
        coefficient_register: u32,
    ) -> Result<(), PaletteError> {
        let start = usize::from(start_register as u8);
        let count = usize::from(count_register as u8);
        check_range("effects", start, count, BANK_COUNT)?;
        for index in 0..count {
            self.effects[start + index] = coefficient_register as u8;
        }
        Ok(())
    }

    /// ROM 0x57c0 uses ff as the cache invalidation byte. If the requested
    /// coefficient is also ff, native processing skips that bank; preserve it.
    pub fn mark_dirty(
        &mut self,
        start_register: u32,
        count_register: u32,
    ) -> Result<(), PaletteError> {
        let start = usize::from(start_register as u8);
        let count = usize::from(count_register as u8);
        check_range("cache", start, count, BANK_COUNT)?;
        for index in 0..count {
            self.previous[start + index] = 0xff;
        }
        Ok(())
    }

    /// ROM 0x5724 copies source colors, not the processed output. It marks
    /// ceil(count/16) banks starting at floor(start/16), ignoring unaligned
    /// start+count crossings. This native dirty-range quirk is retained.
    pub fn copy_source(
        &mut self,
        input: &[u16],
        start_register: u32,
        count_register: u32,
    ) -> Result<usize, PaletteError> {
        let start = usize::from(start_register as u16);
        let count = usize::from(count_register as u16);
        check_range("copy input", 0, count, input.len())?;
        check_range("source", start, count, COLOR_COUNT)?;
        let first_bank = ((start >> 4) & 255) as u32;
        let bank_count = (((count + 15) >> 4) & 255) as u32;
        // Validate every destination before changing either buffer.
        check_range(
            "cache",
            first_bank as usize,
            bank_count as usize,
            BANK_COUNT,
        )?;
        if count == 0 {
            return Ok(0);
        }
        self.mark_dirty(first_bank, bank_count)?;
        self.source[start..start + count].copy_from_slice(&input[..count]);
        Ok(count)
    }

    /// ROM 0x5668 processes only coefficient changes. The native inhibition
    /// halfword and null source pointer gate all work. One bank is 16 colors.
    /// Coefficients 0..32 fade black->source; larger u8 values use reference
    /// white and factor (-coefficient)&31, retaining the native periodic range.
    pub fn process(&mut self, inhibit: u16, source_enabled: bool) -> usize {
        if inhibit != 0 || !source_enabled {
            return 0;
        }
        let mut changed = 0;
        for bank in 0..BANK_COUNT {
            let effect = self.effects[bank];
            if self.previous[bank] == effect {
                continue;
            }
            self.previous[bank] = effect;
            let factor = if effect <= 32 {
                effect
            } else {
                effect.wrapping_neg() & 31
            };
            for index in bank * COLORS_PER_BANK..(bank + 1) * COLORS_PER_BANK {
                let reference = if effect <= 32 {
                    self.black_reference[index]
                } else {
                    self.white_reference[index]
                };
                self.output[index] =
                    blend_native_color(self.source[index], reference, u32::from(factor));
            }
            changed += 1;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Vectors {
        color_vectors: Vec<ColorVector>,
        setter_vectors: Vec<SetterVector>,
        copy_vectors: Vec<CopyVector>,
        update_vectors: Vec<UpdateVector>,
    }

    #[derive(Deserialize)]
    struct ColorVector {
        source: u16,
        reference: u16,
        factor: u32,
        output: u16,
    }

    #[derive(Deserialize)]
    struct SetterVector {
        start: u32,
        count: u32,
        coefficient: u32,
        before: [u8; BANK_COUNT],
        after: [u8; BANK_COUNT],
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CopyVector {
        seed: u32,
        start: u32,
        count: u32,
        previous_before: [u8; BANK_COUNT],
        previous_after: [u8; BANK_COUNT],
        source_sha256: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct UpdateVector {
        seed: u32,
        effects: [u8; BANK_COUNT],
        previous_before: [u8; BANK_COUNT],
        inhibit: u16,
        enabled: bool,
        previous_after: [u8; BANK_COUNT],
        changed_banks: usize,
        output_sha256: String,
    }

    fn vectors() -> Vectors {
        serde_json::from_str(include_str!("../../tests/palette-vectors.json")).unwrap()
    }

    fn synthetic_words(mut seed: u32) -> [u16; COLOR_COUNT] {
        std::array::from_fn(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u16
        })
    }

    fn words_sha256(words: &[u16]) -> String {
        crate::sha256(
            &words
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn scalar_blend_matches_984_original_machine_code_vectors() {
        let vectors = vectors();
        assert_eq!(vectors.color_vectors.len(), 984);
        for vector in vectors.color_vectors {
            assert_eq!(
                blend_native_color(vector.source, vector.reference, vector.factor),
                vector.output
            );
        }
    }

    #[test]
    fn native_setter_truncation_and_copy_cache_ranges_match_reference() {
        let vectors = vectors();
        for vector in vectors.setter_vectors {
            let mut state = PaletteState::new([0; COLOR_COUNT], [0; COLOR_COUNT]);
            state.effects = vector.before;
            state
                .set_effects(vector.start, vector.count, vector.coefficient)
                .unwrap();
            assert_eq!(state.effects, vector.after);
        }
        for vector in vectors.copy_vectors {
            let mut state = PaletteState::new(
                synthetic_words(vector.seed),
                synthetic_words(vector.seed ^ 0xa5a5a5a5),
            );
            state.previous = vector.previous_before;
            let input = synthetic_words(vector.seed ^ 0x55cc33aa);
            state
                .copy_source(&input, vector.start, vector.count)
                .unwrap();
            assert_eq!(state.previous, vector.previous_after);
            assert_eq!(words_sha256(&state.source), vector.source_sha256);
        }
    }

    #[test]
    fn changed_bank_engine_matches_native_outputs_and_inhibition() {
        for vector in vectors().update_vectors {
            let mut state = PaletteState::new(
                synthetic_words(vector.seed),
                synthetic_words(vector.seed ^ 0xa5a5a5a5),
            );
            state.effects = vector.effects;
            state.previous = vector.previous_before;
            assert_eq!(
                state.process(vector.inhibit, vector.enabled),
                vector.changed_banks
            );
            assert_eq!(state.previous, vector.previous_after);
            assert_eq!(words_sha256(&state.output), vector.output_sha256);
        }
    }

    #[test]
    fn invalid_native_ram_corruption_requests_fail_before_mutation() {
        let mut state = PaletteState::new([0; COLOR_COUNT], [0; COLOR_COUNT]);
        let before = state.clone();
        assert!(state.set_effects(31, 2, 32).is_err());
        assert!(state.mark_dirty(32, 1).is_err());
        assert!(state.copy_source(&[7; 16], 511, 2).is_err());
        assert!(state.copy_source(&[], 0, 1).is_err());
        assert_eq!(state, before);
        state.set_effects(255, 256, 123).unwrap();
        state.copy_source(&[], u32::MAX, 0).unwrap();
        assert_eq!(state, before);
    }

    #[test]
    fn raw_copy_and_interpolation_keep_different_bit15_semantics() {
        assert_eq!(blend_native_color(0xffff, 0, 32), 0xffff);
        assert_eq!(blend_native_color(0xffff, 0, 0x10000), 0);
        assert_eq!(blend_native_color(0xffff, 0xffff, 0), 0x7fff);
        let mut destination = [0xabcd; 3];
        blend_into_native(&[0xffff], &[], &mut destination, 0x10001, 0x10001, 32).unwrap();
        assert_eq!(destination, [0xabcd, 0xffff, 0xabcd]);
        assert!(blend_into_native(&[0], &[], &mut destination, 0, 1, 31).is_err());
        assert_eq!(destination, [0xabcd, 0xffff, 0xabcd]);
    }
}
