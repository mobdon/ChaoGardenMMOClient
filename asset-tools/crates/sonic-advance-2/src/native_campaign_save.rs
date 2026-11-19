//! A2NE main-game profile, ten-sector journal and in-memory native save lifecycle.
//!
//! Pinned SAT-R/sa2 save.c/save.h describe this format separately from the
//! embedded garden record. This module never opens a file or device. Hardware
//! programming, interrupts, sound suspension and partial flash damage are not
//! modeled by the explicit memory adapter.

use crate::{Error, bounded_range, gba};
use serde::{Serialize, Serializer};

pub const SECTOR_BYTES: usize = 0x1000;
pub const SECTOR_COUNT: usize = 10;
pub const PAYLOAD_BYTES: usize = 0x378;
pub const PROFILE_BYTES: usize = 0x374;
pub const CHECKSUM_OFFSET: usize = 0x374;
pub const SECURITY: u32 = 0x4547_474d;
pub const TIME_RECORD_COUNT: usize = 315;
pub const NAME_WORDS: usize = 6;
pub const MAX_COURSE_TIME: u16 = 36_000;
pub const NAME_END: u16 = 0xffff;

fn read16(raw: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([raw[offset], raw[offset + 1]])
}
fn read32(raw: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(raw[offset..offset + 4].try_into().expect("bounded field"))
}
fn put16(raw: &mut [u8], offset: usize, value: u16) {
    raw[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(raw: &mut [u8], offset: usize, value: u32) {
    raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn serialize_records<S: Serializer>(
    records: &[u16; TIME_RECORD_COUNT],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    records.as_slice().serialize(serializer)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonConfig {
    pub jump: u16,
    pub attack: u16,
    pub trick: u16,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiplayerScore {
    pub player_id: u32,
    pub player_name: [u16; NAME_WORDS],
    /// Native bool8 bytes are retained; nonzero is true when packing flags.
    pub slot_filled: u8,
    pub wins: u8,
    pub losses: u8,
    pub draws: u8,
}

impl MultiplayerScore {
    fn read(raw: &[u8], offset: usize) -> Self {
        Self {
            player_id: read32(raw, offset),
            player_name: std::array::from_fn(|i| read16(raw, offset + 4 + i * 2)),
            slot_filled: raw[offset + 16],
            wins: raw[offset + 17],
            losses: raw[offset + 18],
            draws: raw[offset + 19],
        }
    }
    fn write(&self, raw: &mut [u8], offset: usize) {
        put32(raw, offset, self.player_id);
        for (i, value) in self.player_name.iter().enumerate() {
            put16(raw, offset + 4 + i * 2, *value);
        }
        raw[offset + 16..offset + 20].copy_from_slice(&[
            self.slot_filled,
            self.wins,
            self.losses,
            self.draws,
        ]);
    }
    fn record_result(&mut self, result: i16) {
        let counter = match result {
            0 => &mut self.wins,
            1 => &mut self.losses,
            2 => &mut self.draws,
            _ => return,
        };
        if *counter < 99 {
            *counter += 1;
        }
    }
}

/// Native in-memory SaveGame fields, not the sector's packed field order.
/// bool8 is represented by u8 to preserve noncanonical native input bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignProfile {
    pub id: u32,
    pub difficulty_level: u8,
    pub time_limit_disabled: u8,
    pub language: u8,
    pub unlocked_levels: [u8; 5],
    pub chaos_emeralds: [u8; 5],
    pub sound_test_unlocked: u8,
    pub boss_time_attack_unlocked: u8,
    pub unlocked_characters: u8,
    pub chao_garden_unlocked: u8,
    pub completed_characters: [u8; 5],
    pub extra_zone_status: u8,
    pub extra_ending_credits_played: u8,
    pub multiplayer_wins: u8,
    pub multiplayer_losses: u8,
    pub multiplayer_draws: u8,
    pub player_name: [u16; NAME_WORDS],
    pub button_config: ButtonConfig,
    #[serde(serialize_with = "serialize_records")]
    pub time_records: [u16; TIME_RECORD_COUNT],
    pub multiplayer_scores: [MultiplayerScore; 10],
    pub score: u32,
    #[serde(skip)]
    padding: [u8; 1],
}

impl CampaignProfile {
    pub fn new_usa() -> Self {
        let mut raw = [0; PROFILE_BYTES];
        raw[6] = 2;
        raw[0x13] = 1;
        put16(&mut raw, 0x20, NAME_END);
        put16(&mut raw, 0x2c, 1);
        put16(&mut raw, 0x2e, 2);
        put16(&mut raw, 0x30, 0x100);
        for i in 0..TIME_RECORD_COUNT {
            put16(&mut raw, 0x32 + i * 2, MAX_COURSE_TIME);
        }
        for i in 0..10 {
            put16(&mut raw, 0x2a8 + i * 20 + 4, NAME_END);
        }
        Self::from_native_bytes(&raw).expect("fixed native initializer")
    }

    pub fn from_native_bytes(raw: &[u8]) -> Result<Self, Error> {
        let raw = bounded_range(raw, 0, PROFILE_BYTES)?;
        Ok(Self {
            id: read32(raw, 0),
            difficulty_level: raw[4],
            time_limit_disabled: raw[5],
            language: raw[6],
            unlocked_levels: raw[7..12].try_into().expect("five bytes"),
            chaos_emeralds: raw[12..17].try_into().expect("five bytes"),
            sound_test_unlocked: raw[17],
            boss_time_attack_unlocked: raw[18],
            unlocked_characters: raw[19],
            chao_garden_unlocked: raw[20],
            completed_characters: raw[21..26].try_into().expect("five bytes"),
            extra_zone_status: raw[26],
            extra_ending_credits_played: raw[27],
            multiplayer_wins: raw[28],
            multiplayer_losses: raw[29],
            multiplayer_draws: raw[30],
            player_name: std::array::from_fn(|i| read16(raw, 0x20 + i * 2)),
            button_config: ButtonConfig {
                jump: read16(raw, 0x2c),
                attack: read16(raw, 0x2e),
                trick: read16(raw, 0x30),
            },
            time_records: std::array::from_fn(|i| read16(raw, 0x32 + i * 2)),
            multiplayer_scores: std::array::from_fn(|i| {
                MultiplayerScore::read(raw, 0x2a8 + i * 20)
            }),
            score: read32(raw, 0x370),
            padding: [raw[0x1f]],
        })
    }

    pub fn native_bytes(&self) -> [u8; PROFILE_BYTES] {
        let mut raw = [0; PROFILE_BYTES];
        put32(&mut raw, 0, self.id);
        raw[4..7].copy_from_slice(&[
            self.difficulty_level,
            self.time_limit_disabled,
            self.language,
        ]);
        raw[7..12].copy_from_slice(&self.unlocked_levels);
        raw[12..17].copy_from_slice(&self.chaos_emeralds);
        raw[17..21].copy_from_slice(&[
            self.sound_test_unlocked,
            self.boss_time_attack_unlocked,
            self.unlocked_characters,
            self.chao_garden_unlocked,
        ]);
        raw[21..26].copy_from_slice(&self.completed_characters);
        raw[26..31].copy_from_slice(&[
            self.extra_zone_status,
            self.extra_ending_credits_played,
            self.multiplayer_wins,
            self.multiplayer_losses,
            self.multiplayer_draws,
        ]);
        for (i, word) in self.player_name.iter().enumerate() {
            put16(&mut raw, 0x20 + i * 2, *word);
        }
        for (i, word) in [
            self.button_config.jump,
            self.button_config.attack,
            self.button_config.trick,
        ]
        .iter()
        .enumerate()
        {
            put16(&mut raw, 0x2c + i * 2, *word);
        }
        for (i, word) in self.time_records.iter().enumerate() {
            put16(&mut raw, 0x32 + i * 2, *word);
        }
        for (i, entry) in self.multiplayer_scores.iter().enumerate() {
            entry.write(&mut raw, 0x2a8 + i * 20);
        }
        put32(&mut raw, 0x370, self.score);
        for (offset, value) in [0x1f].into_iter().zip(self.padding) {
            raw[offset] = value;
        }
        raw
    }

    /// Native debug/completed profile initializer; it is no MMO unlock grant.
    pub fn generate_completed(&mut self, random32: &mut impl FnMut() -> u32) {
        if self.id == 0 {
            self.id = random32();
        }
        self.score = 0;
        self.unlocked_levels = [30, 29, 29, 29, 29];
        self.chaos_emeralds = [255; 5];
        self.unlocked_characters = 31;
        self.sound_test_unlocked = 1;
        self.boss_time_attack_unlocked = 1;
        self.chao_garden_unlocked = 1;
        self.completed_characters = [1; 5];
        self.extra_zone_status = 2;
        self.extra_ending_credits_played = 1;
    }

    pub fn insert_multiplayer_profile(&mut self, player_id: u32, player_name: [u16; NAME_WORDS]) {
        if let Some(index) = self
            .multiplayer_scores
            .iter()
            .position(|score| score.player_id == player_id && score.player_name == player_name)
        {
            self.multiplayer_scores[..=index].rotate_right(1);
        } else {
            self.multiplayer_scores.rotate_right(1);
            self.multiplayer_scores[0] = MultiplayerScore {
                player_id,
                player_name,
                slot_filled: 1,
                wins: 0,
                losses: 0,
                draws: 0,
            };
        }
    }
    pub fn record_own_result(&mut self, result: i16) {
        let counter = match result {
            0 => &mut self.multiplayer_wins,
            1 => &mut self.multiplayer_losses,
            2 => &mut self.multiplayer_draws,
            _ => return,
        };
        if *counter < 99 {
            *counter += 1;
        }
    }
    pub fn record_multiplayer_result(
        &mut self,
        player_id: u32,
        name: [u16; NAME_WORDS],
        result: i16,
    ) {
        if let Some(score) = self
            .multiplayer_scores
            .iter_mut()
            .find(|score| score.player_id == player_id && score.player_name == name)
        {
            score.record_result(result);
        }
    }
}

fn packed_button(value: u16) -> u8 {
    match value {
        1 => 1,
        2 => 2,
        0x100 => 4,
        _ => 0,
    }
}
fn unpacked_button(value: u8) -> u16 {
    match value {
        1 => 1,
        2 => 2,
        4 => 0x100,
        _ => 0,
    }
}

pub fn checksum(payload: &[u8]) -> Result<u32, Error> {
    Ok(bounded_range(payload, 0, CHECKSUM_OFFSET)?
        .as_chunks::<4>()
        .0
        .iter()
        .fold(0u32, |sum, word| {
            sum.wrapping_add(u32::from_le_bytes(*word))
        }))
}

pub fn checksum_valid(payload: &[u8]) -> Result<bool, Error> {
    let payload = bounded_range(payload, 0, PAYLOAD_BYTES)?;
    Ok(read32(payload, CHECKSUM_OFFSET) == checksum(payload)?)
}

/// Native PackSaveSectorData. The supplied word is the caller's Random32
/// output; the format routine neither invents entropy nor trusts game gains.
pub fn pack_sector(
    profile: &CampaignProfile,
    previous_version: u32,
    random_word: u32,
) -> [u8; PAYLOAD_BYTES] {
    let mut raw = [0; PAYLOAD_BYTES];
    put32(&mut raw, 0, SECURITY);
    put32(&mut raw, 4, previous_version.wrapping_add(1));
    put32(&mut raw, 8, profile.id);
    for (i, value) in profile.player_name.iter().enumerate() {
        put16(&mut raw, 0xc + i * 2, *value);
    }
    raw[0x18] = profile.language;
    raw[0x19] =
        u8::from(profile.difficulty_level != 0) | (u8::from(profile.time_limit_disabled != 0) << 1);
    raw[0x1a] = u8::from(profile.completed_characters[4] != 0)
        | (profile.extra_zone_status.wrapping_shl(1) & 6)
        | (u8::from(profile.extra_ending_credits_played != 0) << 3);
    for (i, completed) in profile.completed_characters.iter().take(4).enumerate() {
        raw[0x1a] |= u8::from(*completed != 0) << (4 + i);
    }
    raw[0x1b] = ((profile.unlocked_characters >> 1) & 15)
        | (u8::from(profile.boss_time_attack_unlocked != 0) << 4)
        | (u8::from(profile.sound_test_unlocked != 0) << 5)
        | (u8::from(profile.chao_garden_unlocked != 0) << 6);
    raw[0x1c..0x1f].copy_from_slice(&[
        packed_button(profile.button_config.jump),
        packed_button(profile.button_config.attack),
        packed_button(profile.button_config.trick),
    ]);
    raw[0x1f..0x24].copy_from_slice(&profile.unlocked_levels);
    raw[0x24..0x29].copy_from_slice(&profile.chaos_emeralds);
    raw[0x29..0x2c].copy_from_slice(&[
        profile.multiplayer_wins,
        profile.multiplayer_losses,
        profile.multiplayer_draws,
    ]);
    for (i, value) in profile.time_records.iter().enumerate() {
        put16(&mut raw, 0x2c + i * 2, *value);
    }
    for (i, entry) in profile.multiplayer_scores.iter().enumerate() {
        entry.write(&mut raw, 0x2a4 + i * 20);
    }
    put32(&mut raw, 0x36c, random_word);
    put32(&mut raw, 0x370, profile.score);
    let sum = checksum(&raw).expect("fixed native payload");
    put32(&mut raw, CHECKSUM_OFFSET, sum);
    raw
}

/// Native UnpackSaveSectorData performs no signature/checksum validation.
/// Callers inspecting untrusted bytes must check checksum_valid separately.
pub fn unpack_sector(payload: &[u8]) -> Result<CampaignProfile, Error> {
    let raw = bounded_range(payload, 0, PAYLOAD_BYTES)?;
    let mut profile = CampaignProfile::new_usa();
    profile.id = read32(raw, 8);
    profile.player_name = std::array::from_fn(|i| read16(raw, 0xc + i * 2));
    profile.language = raw[0x18];
    profile.difficulty_level = raw[0x19] & 1;
    profile.time_limit_disabled = u8::from(raw[0x19] & 2 != 0);
    profile.completed_characters =
        std::array::from_fn(|i| u8::from(raw[0x1a] & if i == 4 { 1 } else { 1 << (4 + i) } != 0));
    profile.extra_zone_status = (raw[0x1a] & 6) >> 1;
    profile.extra_ending_credits_played = u8::from(raw[0x1a] & 8 != 0);
    profile.unlocked_characters = 1 | ((raw[0x1b] & 15) << 1);
    profile.sound_test_unlocked = u8::from(raw[0x1b] & 32 != 0);
    profile.boss_time_attack_unlocked = u8::from(raw[0x1b] & 16 != 0);
    profile.chao_garden_unlocked = u8::from(raw[0x1b] & 64 != 0);
    profile.button_config = ButtonConfig {
        jump: unpacked_button(raw[0x1c]),
        attack: unpacked_button(raw[0x1d]),
        trick: unpacked_button(raw[0x1e]),
    };
    profile.unlocked_levels.copy_from_slice(&raw[0x1f..0x24]);
    profile.chaos_emeralds.copy_from_slice(&raw[0x24..0x29]);
    profile.multiplayer_wins = raw[0x29];
    profile.multiplayer_losses = raw[0x2a];
    profile.multiplayer_draws = raw[0x2b];
    profile.time_records = std::array::from_fn(|i| read16(raw, 0x2c + i * 2));
    profile.multiplayer_scores =
        std::array::from_fn(|i| MultiplayerScore::read(raw, 0x2a4 + i * 20));
    profile.score = read32(raw, 0x370);
    Ok(profile)
}

fn validate_flash(flash: &[u8]) -> Result<(), Error> {
    if flash.len() < SECTOR_COUNT * SECTOR_BYTES
        || flash.len() > 16 * SECTOR_BYTES
        || !flash.len().is_multiple_of(SECTOR_BYTES)
    {
        return Err(Error(
            "Campaign flash requires 10..16 complete 4-KiB sectors".into(),
        ));
    }
    Ok(())
}

pub fn find_newest_sector(flash: &[u8]) -> Result<usize, Error> {
    validate_flash(flash)?;
    let (mut max_version, mut min_version, mut best) = (0u32, u32::MAX, 0);
    for sector in 0..SECTOR_COUNT {
        let offset = sector * SECTOR_BYTES;
        if read32(flash, offset) == SECURITY {
            let version = read32(flash, offset + 4);
            if version > max_version {
                max_version = version;
                best = sector;
            }
            min_version = min_version.min(version);
        }
    }
    if min_version > 0 {
        return Ok(best);
    }
    max_version = 0;
    // Native rollover's second pass intentionally does not recheck security.
    for sector in 0..SECTOR_COUNT {
        let version = read32(flash, sector * SECTOR_BYTES + 4);
        if max_version <= 0xffff && version > max_version {
            best = sector;
            max_version = version;
        }
    }
    Ok(best)
}

pub fn find_oldest_sector(flash: &[u8]) -> Result<Option<usize>, Error> {
    validate_flash(flash)?;
    let (mut max_version, mut min_version, mut best) = (0u32, u32::MAX, None);
    for sector in 0..SECTOR_COUNT {
        let offset = sector * SECTOR_BYTES;
        if read32(flash, offset) != SECURITY {
            return Ok(Some(sector));
        }
        let version = read32(flash, offset + 4);
        max_version = max_version.max(version);
        if version < min_version {
            best = Some(sector);
            min_version = version;
        }
    }
    if max_version != u32::MAX {
        return Ok(best);
    }
    min_version = 0xffff0000;
    for sector in 0..SECTOR_COUNT {
        let version = read32(flash, sector * SECTOR_BYTES + 4);
        if min_version > 0xffff0000 && version < min_version {
            best = Some(sector);
            min_version = version;
        }
    }
    Ok(if min_version != 0xffff0000 {
        best
    } else {
        None
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorStatus {
    pub sector: usize,
    pub security: u32,
    pub version: u32,
    pub native_header_present: bool,
    pub checksum_valid: bool,
    pub stored_checksum: u32,
    pub calculated_checksum: u32,
    pub payload_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignFlashReport {
    pub bytes: usize,
    pub header_save_exists: bool,
    pub newest_candidate: usize,
    pub oldest_candidate: Option<usize>,
    pub sectors: Vec<SectorStatus>,
}

pub fn inspect_flash(flash: &[u8]) -> Result<CampaignFlashReport, Error> {
    validate_flash(flash)?;
    let mut sectors = Vec::with_capacity(SECTOR_COUNT);
    for sector in 0..SECTOR_COUNT {
        let payload = &flash[sector * SECTOR_BYTES..sector * SECTOR_BYTES + PAYLOAD_BYTES];
        sectors.push(SectorStatus {
            sector,
            security: read32(payload, 0),
            version: read32(payload, 4),
            native_header_present: read32(payload, 0) == SECURITY,
            checksum_valid: checksum_valid(payload)?,
            stored_checksum: read32(payload, CHECKSUM_OFFSET),
            calculated_checksum: checksum(payload)?,
            payload_sha256: gba::sha256(payload),
        });
    }
    Ok(CampaignFlashReport {
        bytes: flash.len(),
        header_save_exists: sectors.iter().any(|s| s.native_header_present),
        newest_candidate: find_newest_sector(flash)?,
        oldest_candidate: find_oldest_sector(flash)?,
        sectors,
    })
}

/// Failure codes belong to the memory test adapter, not a physical flash model.
/// A failed operation leaves that sector unchanged; partial damage is unmodeled.
#[derive(Debug, Default, Clone)]
pub struct MemoryFlashFaults {
    pub program_error: u16,
    pub erase_errors: [u16; SECTOR_COUNT],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EraseAttempt {
    pub sector: usize,
    pub error: u16,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadReport {
    pub success: bool,
    pub loaded_sector: Option<usize>,
    pub erase_attempts: Vec<EraseAttempt>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    pub success: bool,
    pub skipped_unchanged: bool,
    pub written_sector: Option<usize>,
    pub error: u16,
    pub erase_attempts: Vec<EraseAttempt>,
}

pub struct CampaignSaveSession {
    flash: Vec<u8>,
    pub loaded: CampaignProfile,
    last_written: CampaignProfile,
    sector_buffer: [u8; PAYLOAD_BYTES],
    pub flash_present: bool,
}

impl CampaignSaveSession {
    pub fn new(flash: &[u8], flash_present: bool) -> Result<Self, Error> {
        validate_flash(flash)?;
        let loaded = CampaignProfile::new_usa();
        let mut sector_buffer = pack_sector(&loaded, u32::MAX, 0);
        put32(&mut sector_buffer, CHECKSUM_OFFSET, 0); // InitSaveGameSectorData does not calculate one.
        Ok(Self {
            flash: flash.to_vec(),
            last_written: loaded.clone(),
            loaded,
            sector_buffer,
            flash_present,
        })
    }
    pub fn flash_bytes(&self) -> &[u8] {
        &self.flash
    }
    pub fn last_written(&self) -> &CampaignProfile {
        &self.last_written
    }
    pub fn sector_buffer(&self) -> &[u8; PAYLOAD_BYTES] {
        &self.sector_buffer
    }
    pub fn has_changes(&self) -> bool {
        self.loaded != self.last_written
    }
    pub fn save_exists(&self) -> bool {
        self.flash_present
            && (0..SECTOR_COUNT)
                .any(|sector| read32(&self.flash, sector * SECTOR_BYTES) == SECURITY)
    }
    fn erase(&mut self, sector: usize, faults: &MemoryFlashFaults) -> EraseAttempt {
        let error = faults.erase_errors[sector];
        if error == 0 {
            self.flash[sector * SECTOR_BYTES..(sector + 1) * SECTOR_BYTES].fill(0xff);
        }
        EraseAttempt { sector, error }
    }
    fn program(&mut self, sector: usize, faults: &MemoryFlashFaults) -> u16 {
        if faults.program_error != 0 {
            return faults.program_error;
        }
        // Explicit memory binding: unused target-sector bytes become erased FF.
        // This does not reproduce a physical driver's out-of-payload writes.
        self.flash[sector * SECTOR_BYTES..(sector + 1) * SECTOR_BYTES].fill(0xff);
        self.flash[sector * SECTOR_BYTES..sector * SECTOR_BYTES + PAYLOAD_BYTES]
            .copy_from_slice(&self.sector_buffer);
        0
    }
    pub fn load_latest(&mut self, faults: &MemoryFlashFaults) -> LoadReport {
        self.last_written = self.loaded.clone();
        let mut report = LoadReport {
            success: false,
            loaded_sector: None,
            erase_attempts: Vec::new(),
        };
        if !self.flash_present {
            return report;
        }
        let mut sector = find_newest_sector(&self.flash).expect("validated flash");
        for _ in 0..SECTOR_COUNT {
            self.sector_buffer.copy_from_slice(
                &self.flash[sector * SECTOR_BYTES..sector * SECTOR_BYTES + PAYLOAD_BYTES],
            );
            if checksum_valid(&self.sector_buffer).expect("fixed payload") {
                self.loaded = unpack_sector(&self.sector_buffer).expect("fixed payload");
                self.last_written = self.loaded.clone();
                report.success = true;
                report.loaded_sector = Some(sector);
                return report;
            }
            report.erase_attempts.push(self.erase(sector, faults));
            sector = if sector == 0 {
                SECTOR_COUNT - 1
            } else {
                sector - 1
            };
        }
        report
    }
    pub fn write(
        &mut self,
        random32: &mut impl FnMut() -> u32,
        faults: &MemoryFlashFaults,
    ) -> WriteReport {
        let mut report = WriteReport {
            success: false,
            skipped_unchanged: false,
            written_sector: None,
            error: 0,
            erase_attempts: Vec::new(),
        };
        if !self.has_changes() && self.loaded.id != 0 {
            report.success = true;
            report.skipped_unchanged = true;
            return report;
        }
        if self.loaded.id == 0 {
            self.loaded.id = random32();
        }
        self.last_written = self.loaded.clone();
        self.sector_buffer = pack_sector(
            &self.last_written,
            read32(&self.sector_buffer, 4),
            random32(),
        );
        if !self.flash_present {
            return report;
        }
        let sector = find_oldest_sector(&self.flash)
            .expect("validated flash")
            .unwrap_or(0);
        report.written_sector = Some(sector);
        report.error = self.program(sector, faults);
        report.success = report.error == 0;
        report
    }
    /// Native NewSaveGame resets only the copied memory image. It preserves
    /// current language/score and touches only campaign sectors 0..9.
    pub fn new_save(
        &mut self,
        random32: &mut impl FnMut() -> u32,
        faults: &MemoryFlashFaults,
    ) -> WriteReport {
        let language = self.loaded.language;
        let score = self.loaded.score;
        self.loaded = CampaignProfile::new_usa();
        self.loaded.language = language;
        self.loaded.score = score;
        self.last_written = self.loaded.clone();
        let mut report = WriteReport {
            success: false,
            skipped_unchanged: false,
            written_sector: None,
            error: 0,
            erase_attempts: Vec::new(),
        };
        if !self.flash_present {
            return report;
        }
        self.sector_buffer = pack_sector(&self.last_written, 0, random32());
        report.written_sector = Some(0);
        report.error = self.program(0, faults);
        if report.error != 0 {
            return report;
        }
        for sector in 1..SECTOR_COUNT {
            let attempt = self.erase(sector, faults);
            report.error = attempt.error;
            report.erase_attempts.push(attempt);
            if report.error != 0 {
                return report;
            }
        }
        report.success = true;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_packing_normalizes_flags_and_preserves_all_native_arrays() {
        let mut profile = CampaignProfile::new_usa();
        profile.player_name = [1, 2, 3, 4, 5, 6];
        profile.time_records = std::array::from_fn(|i| i as u16);
        profile.completed_characters = [2, 3, 4, 5, 6];
        profile.extra_zone_status = 255;
        profile.button_config.jump = 0x101;
        let payload = pack_sector(&profile, u32::MAX, 0x12345678);
        assert!(checksum_valid(&payload).unwrap());
        assert_eq!(read32(&payload, 4), 0);
        let parsed = unpack_sector(&payload).unwrap();
        assert_eq!(parsed.player_name, profile.player_name);
        assert_eq!(parsed.time_records, profile.time_records);
        assert_eq!(parsed.completed_characters, [1; 5]);
        assert_eq!(parsed.extra_zone_status, 3);
        assert_eq!(parsed.button_config.jump, 0);
        assert!(unpack_sector(&payload[..PAYLOAD_BYTES - 1]).is_err());
    }

    #[test]
    fn rollover_and_checksum_only_recovery_keep_native_quirks() {
        let mut flash = vec![0xff; 16 * SECTOR_BYTES];
        let profile = CampaignProfile::new_usa();
        for sector in 0..SECTOR_COUNT {
            let payload = pack_sector(&profile, sector as u32, 0);
            flash[sector * SECTOR_BYTES..sector * SECTOR_BYTES + PAYLOAD_BYTES]
                .copy_from_slice(&payload);
        }
        put32(&mut flash, 4, u32::MAX);
        assert_eq!(find_oldest_sector(&flash).unwrap(), None);
        put32(&mut flash, 4, 0);
        put32(&mut flash, SECTOR_BYTES, 0);
        put32(&mut flash, SECTOR_BYTES + 4, u32::MAX);
        assert_eq!(
            find_newest_sector(&flash).unwrap(),
            1,
            "rollover second pass ignores signature"
        );
        flash[2 * SECTOR_BYTES..10 * SECTOR_BYTES].fill(0xff);
        put32(&mut flash, SECTOR_BYTES, SECURITY);
        flash[..PAYLOAD_BYTES].fill(0); // valid checksum despite absent signature
        let mut session = CampaignSaveSession::new(&flash, true).unwrap();
        let loaded = session.load_latest(&MemoryFlashFaults::default());
        assert!(loaded.success);
        assert_eq!(loaded.loaded_sector, Some(0));
        assert_eq!(loaded.erase_attempts.len(), 1);
        assert_eq!(
            &session.flash_bytes()[10 * SECTOR_BYTES..],
            &flash[10 * SECTOR_BYTES..]
        );
    }

    #[test]
    fn failed_write_updates_native_cached_state_and_new_save_preserves_garden() {
        let flash = vec![0xa5; 16 * SECTOR_BYTES];
        let mut session = CampaignSaveSession::new(&flash, true).unwrap();
        let mut counter = 0u32;
        let mut random = || {
            counter += 1;
            counter
        };
        let failed = session.write(
            &mut random,
            &MemoryFlashFaults {
                program_error: 7,
                ..Default::default()
            },
        );
        assert!(!failed.success);
        assert_eq!(session.loaded.id, 1);
        assert!(!session.has_changes());
        let skipped = session.write(&mut random, &MemoryFlashFaults::default());
        assert!(skipped.success && skipped.skipped_unchanged);
        assert_eq!(session.flash_bytes(), flash);
        session.loaded.language = 6;
        session.loaded.score = 123;
        assert!(
            session
                .new_save(&mut random, &MemoryFlashFaults::default())
                .success
        );
        assert_eq!(
            (
                session.loaded.id,
                session.loaded.language,
                session.loaded.score
            ),
            (0, 6, 123)
        );
        assert_eq!(
            &session.flash_bytes()[10 * SECTOR_BYTES..],
            &flash[10 * SECTOR_BYTES..]
        );
    }
}
