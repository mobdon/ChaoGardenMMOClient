//! The MMO server memory card: signing in inserts it. With `SA2_MMO_SERVER`
//! set, slot A is present only while a session token is there (from the
//! server's `/api/session`, in `SA2_MMO_TOKEN` or the file
//! `SA2_MMO_TOKEN_FILE`), and the Chao garden file is the account's save on
//! the server: mounting checks it out (`GET
//! /api/game-save`), and every write of it sends the whole file back (`PUT`).
//! The bytes are exactly what the game wrote. Other card files stay in the
//! local `SA2_MEMCARD` folder, which works as the cache.
//!
//! Desktop only: the browser build has no card folder and bridges this
//! through its page instead.
use sa2_recomp::{global, global::Global};
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

/// The Chao garden save on the card. Source: ChaoMain card code (GSNE8P),
/// as read by the Race/Karate thread; the live trace confirms it.
pub const GARDEN_FILE: &str = "SONIC2B__ALF";

/// A whole Memory Card 59: the server's limit for one card file.
const MAX_BYTES: u64 = 59 * 8192;

/// The server keeps a lease 10 minutes; renew well inside that.
const RENEW_EVERY: Duration = Duration::from_secs(120);

struct Session {
    lease: String,
    revision: u64,
    renewed: Instant,
}

static SESSION: Global<Option<Session>> = global!(None);

fn server() -> Option<String> {
    std::env::var("SA2_MMO_SERVER")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_owned())
}

/// The session token: `SA2_MMO_TOKEN`, or the file `SA2_MMO_TOKEN_FILE`
/// names, re-read on each check so signing in while the game runs inserts
/// the card and the game's own card checks see it arrive.
fn token() -> Option<String> {
    std::env::var("SA2_MMO_TOKEN")
        .ok()
        .or_else(|| std::fs::read_to_string(std::env::var_os("SA2_MMO_TOKEN_FILE")?).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// True when the card is the MMO server card.
pub fn configured() -> bool {
    server().is_some()
}

/// The server card is in the slot only while signed in.
pub fn signed_in() -> bool {
    token().is_some()
}

fn auth() -> String {
    format!("Bearer {}", token().unwrap_or_default())
}

fn trace(what: &str) {
    if std::env::var_os("SA2_TRACE_CARD").is_some() {
        eprintln!("card (mmo): {what}");
    }
}

/// Check the garden save out into `path` (the local cache file), once per run.
/// A server with no save yet keeps the local file, which the game's next
/// write uploads.
pub fn pull(path: &Path) -> Result<(), String> {
    if SESSION.with_borrow(|s| s.is_some()) {
        return Ok(());
    }
    let url = format!("{}/api/game-save", server().ok_or("no server")?);
    let response = ureq::get(&url)
        .set("Authorization", &auth())
        .call()
        .map_err(|e| e.to_string())?;
    let revision = response
        .header("x-save-revision")
        .and_then(|v| v.parse().ok())
        .ok_or("no save revision")?;
    let lease = response
        .header("x-save-lease")
        .ok_or("no lease")?
        .to_owned();
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if !bytes.is_empty() {
        std::fs::write(path, &bytes).map_err(|e| e.to_string())?;
    }
    trace(&format!(
        "checked out revision {revision}, {} bytes",
        bytes.len()
    ));
    SESSION.set(Some(Session {
        lease,
        revision,
        renewed: Instant::now(),
    }));
    Ok(())
}

/// Send the whole garden file after the game wrote it.
pub fn push(bytes: &[u8]) -> Result<(), String> {
    let (lease, revision) = SESSION
        .with_borrow(|s| s.as_ref().map(|s| (s.lease.clone(), s.revision)))
        .ok_or("save not checked out")?;
    let url = format!("{}/api/game-save", server().ok_or("no server")?);
    ureq::put(&url)
        .set("Authorization", &auth())
        .set("x-save-lease", &lease)
        .set("x-save-revision", &revision.to_string())
        .send_bytes(bytes)
        .map_err(|e| e.to_string())?;
    // The server stores an accepted write as the next revision.
    let next = revision + 1;
    SESSION.with_borrow_mut(|s| {
        if let Some(s) = s {
            s.revision = next;
            s.renewed = Instant::now();
        }
    });
    trace(&format!("saved revision {next}, {} bytes", bytes.len()));
    Ok(())
}

/// Frame hook: keep the lease while the game runs (off the game thread).
pub fn renew_if_due() {
    let Some(lease) = SESSION.with_borrow_mut(|s| {
        let s = s.as_mut()?;
        if s.renewed.elapsed() < RENEW_EVERY {
            return None;
        }
        s.renewed = Instant::now();
        Some(s.lease.clone())
    }) else {
        return;
    };
    let (Some(server), auth) = (server(), auth()) else {
        return;
    };
    std::thread::spawn(move || {
        let result = ureq::post(&format!("{server}/api/game-save/lease"))
            .set("Authorization", &auth)
            .set("x-save-lease", &lease)
            .call();
        if let Err(e) = result {
            eprintln!("card (mmo): lease renewal failed: {e}");
        }
    });
}

/// The game is closing: free the Chao for the website and Tiny Garden now
/// instead of when the lease runs out.
pub fn release() {
    let Some(lease) = SESSION.take().map(|s| s.lease) else {
        return;
    };
    let Some(server) = server() else { return };
    let result = ureq::delete(&format!("{server}/api/game-save/lease"))
        .set("Authorization", &auth())
        .set("x-save-lease", &lease)
        .call();
    if let Err(e) = result {
        eprintln!("card (mmo): could not release the garden save: {e}");
    }
}
