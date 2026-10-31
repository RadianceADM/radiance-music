//! Who's asking: every API call carries its device's token
//! (`Authorization: Bearer TOKEN`). Players and browsers can't set headers on
//! a file or a WebSocket, so those take `?t=TOKEN` too. No passwords: each
//! device gets its own token when it joins (like pairing), and the owner can
//! take it back. Only the token's SHA-256 is kept.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::{now_ms, ApiError, Shared};

/// A joined device and its person.
#[derive(Clone, Debug)]
pub struct Who {
    pub person: i64,
    pub name: String,
    pub owner: bool,
    pub device: i64,
}

pub fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn random(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).expect("the system's random numbers");
    buf
}

/// A device's token: 32 random bytes, as hex.
pub fn new_token() -> String {
    random(32).iter().map(|b| format!("{b:02x}")).collect()
}

/// Letters that can't be mixed up when read out or typed (no 0/O, 1/I/L).
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// An invite's one-time code: 12 letters (about 59 bits), shown as XXXX-XXXX-XXXX.
pub fn new_code() -> String {
    random(12).iter().map(|b| CODE_LETTERS[*b as usize % CODE_LETTERS.len()] as char).collect()
}

/// A code as typed or pasted (dashes, spaces, small letters), as kept.
pub fn plain_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// A code as shown: XXXX-XXXX-XXXX.
pub fn show_code(code: &str) -> String {
    code.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-")
}

/// The device a token belongs to, if it's still joined (and its person not removed).
pub fn lookup(app: &Shared, token: &str) -> Option<Who> {
    if token.is_empty() {
        return None;
    }
    let conn = app.db();
    let who = conn
        .query_row(
            "SELECT d.id, p.id, p.name, p.role, d.seen_ms FROM devices d JOIN people p ON p.id = d.person_id
             WHERE d.token_sha256 = ?1 AND d.revoked_ms IS NULL AND p.revoked_ms IS NULL",
            params![hash(token)],
            |r| Ok((Who { device: r.get(0)?, person: r.get(1)?, name: r.get(2)?, owner: r.get::<_, String>(3)? == "owner" }, r.get::<_, Option<i64>>(4)?)),
        )
        .optional()
        .ok()
        .flatten();
    let (who, seen) = who?;
    // when it was last seen, to the minute (not a write for every file range)
    let now = now_ms();
    if seen.is_none_or(|s| now - s > 60_000) {
        let _ = conn.execute("UPDATE devices SET seen_ms = ?1 WHERE id = ?2", params![now, who.device]);
    }
    Some(who)
}

fn bearer(parts: &Parts) -> Option<String> {
    let h = parts.headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let t = h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer "))?;
    Some(t.trim().to_string())
}

/// `?t=TOKEN` in the address.
pub fn query_token(parts: &Parts) -> Option<String> {
    parts.uri.query()?.split('&').find_map(|kv| kv.strip_prefix("t=")).map(|t| percent_encoding::percent_decode_str(t).decode_utf8_lossy().into_owned())
}

fn signed_out(had_token: bool) -> ApiError {
    ApiError(
        StatusCode::UNAUTHORIZED,
        if had_token {
            "This device isn't joined to the server, or it was removed. Ask the server's owner for a new invite.".into()
        } else {
            "This needs your device's key (Authorization: Bearer …). Join the server with an invite first.".into()
        },
    )
}

fn who(parts: &Parts, app: &Shared, query_too: bool) -> Result<Who, ApiError> {
    let token = bearer(parts).or_else(|| if query_too { query_token(parts) } else { None });
    let had = token.is_some();
    token.and_then(|t| lookup(app, &t)).ok_or_else(|| signed_out(had))
}

impl FromRequestParts<Shared> for Who {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Shared) -> Result<Self, Self::Rejection> {
        who(parts, app, false)
    }
}

/// A file or the party's WebSocket: the header, or `?t=TOKEN`.
pub struct Listener(pub Who);

impl FromRequestParts<Shared> for Listener {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Shared) -> Result<Self, Self::Rejection> {
        who(parts, app, true).map(Listener)
    }
}

/// The server's owner only (invites, people, devices).
pub struct Owner(pub Who);

impl FromRequestParts<Shared> for Owner {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Shared) -> Result<Self, Self::Rejection> {
        let w = who(parts, app, false)?;
        if !w.owner {
            return Err(crate::forbidden("Only the server's owner can do that."));
        }
        Ok(Owner(w))
    }
}
