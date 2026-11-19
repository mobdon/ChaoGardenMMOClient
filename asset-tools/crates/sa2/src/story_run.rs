//! Headless local story dispatch. A level always remains pending.
use super::{Extra, Result};
use sa2_asset_tools::{event, story, Error};
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn run(
    exe: &[u8],
    install: &Path,
    extras: &mut Vec<Extra>,
    name: &str,
) -> Result<Value> {
    let sequences = story::read(exe, extras[0].text()?)?;
    let sequence = sequences
        .get(name)
        .ok_or_else(|| Error::invalid("unknown story sequence"))?;
    let mut cursor = story::Cursor::new(sequence);
    let mut events = Vec::new();
    let mut byte_total = 0usize;
    let stop = loop {
        let Some(d) = cursor.dispatch() else {
            break json!({"kind":"end"});
        };
        let stop = |reason| json!({"kind":reason,"action":d.action,"actionIndex":events.len(),"entryRva":d.source_rva});
        let story::Action::Event { event: id } = d.action else {
            break stop(match d.action {
                story::Action::Level { .. } => "awaitingLevelRuntime",
                _ => "awaitingCredits",
            });
        };
        let logical = format!("resource/gd_PC/event/e{id:04}.prs");
        let path = install.join(&logical);
        if !path.try_exists()? {
            if sa2_asset_tools::story_movie::supported(id) {
                let movie = format!("resource/gd_PC/event/e{id:04}_m.sfd");
                let path = install.join(&movie);
                if path.try_exists()? {
                    let src = Extra::read(
                        &path,
                        movie,
                        (128usize * 1024 * 1024 - byte_total).min(80 * 1024 * 1024) as u64,
                        "story-event-movie",
                    )?;
                    let mut result = stop("awaitingMoviePlayback");
                    result["movie"] = sa2_asset_tools::story_movie::inspect_movie(id, &src.bytes)?;
                    extras.push(src);
                    break result;
                }
            }
            break stop("missingMainEvent");
        }
        let remaining = (128usize * 1024 * 1024).saturating_sub(byte_total);
        let src = Extra::read(
            &path,
            logical,
            remaining.min(64 * 1024 * 1024) as u64,
            "story-event-image",
        )?;
        byte_total += src.bytes.len();
        let clip = event::read_prs(&src.bytes)?;
        let frames = clip.scenes[0].frames;
        let last = clip.frame(
            frames
                .checked_sub(1)
                .ok_or_else(|| Error::invalid("empty event timeline"))?,
        )?;
        events.push(json!({"event":id,"frames":frames,"lastFrame":last,"entryRva":d.source_rva}));
        extras.push(src);
        cursor.acknowledge(&d.token)?;
    };
    Ok(
        json!({"sequence":name,"events":events,"stop":stop,"mode":"headless event timeline; no rendering, level clears, save writes or Chao rewards"}),
    )
}
