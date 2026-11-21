//! The MMO server memory card: signing in inserts it. With `SA2_MMO_SERVER`
//! set, slot A is present only while a session token is there (from the
//! server's `/api/session`, in `SA2_MMO_TOKEN` or the file
//! `SA2_MMO_TOKEN_FILE`), and the whole card is the account's save on the
//! server: mounting checks it out (`GET /api/game-save`) into the local
//! `SA2_MEMCARD` folder (the cache), and every change the game makes to a
//! card file sends the whole card back (`PUT`). File bytes are exactly what
//! the game wrote.
//! The card travels packed as `crate::card_pack` describes.
//!
//! Desktop only: the browser build has no card folder and bridges this
//! through its page instead.
use sa2_recomp::{global, global::Global};
use std::io::Read;
use std::time::{Duration, Instant};

/// A whole Memory Card 59 (59 blocks of data) plus room for the file names
/// and status: the server's limit for one card.
const MAX_BYTES: u64 = 65 * 8192;

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

/// Check the card out, once per run, and hand its files to `store`. A server
/// with no card yet keeps the local files, which the game's next change
/// uploads.
pub fn pull(
    store: impl FnOnce(Vec<crate::card_pack::CardFile>) -> Result<(), String>,
) -> Result<(), String> {
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
        store(crate::card_pack::unpack(&bytes).ok_or("the server card is damaged")?)?;
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

/// Send the whole card after the game changed it.
pub fn push(files: &[crate::card_pack::CardFile]) -> Result<(), String> {
    let bytes = &crate::card_pack::pack(files);
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
