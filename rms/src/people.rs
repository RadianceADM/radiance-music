//! The owner and the friends they invite. The first person (from the setup
//! page) owns the server; every friend joins with an invite the owner makes,
//! one per friend, one-time, and can be removed any time. Each device that
//! joins gets its own token.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Owner, Who};
use crate::db::INVITE_MS;
use crate::{bad, conflict, forbidden, not_found, now_ms, web, ApiError, ApiResult, Shared};

const KEY_MISSING: &str = "Type the setup key first. It's the key you typed when deploying; if you didn't type one: Hostinger → Docker Manager → your project → Logs, or the file setup-key.txt in the server's data.";
const KEY_WRONG: &str = "That setup key isn't right. It's the key you typed when deploying; if you didn't type one: Hostinger → Docker Manager → your project → Logs, or the file setup-key.txt in the server's data.";

/// Names are kept short (they show on every friend's screen).
const MOST_NAME: usize = 40;

fn clean_name(name: &str, most: usize) -> String {
    name.chars().filter(|c| !c.is_control()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(most).collect()
}

/// What anyone may know (before joining too): the server's name, whether it's
/// set up, and how the splitter is getting on (the models' download shows on
/// the setup page).
pub async fn status(State(app): State<Shared>, headers: HeaderMap) -> Json<Value> {
    let name = app.server_name();
    let base = web::base_url(&app, &headers);
    let https = base.starts_with("https://");
    let host = base.split("://").nth(1).unwrap_or("").to_string();
    Json(json!({
        "server": name,
        "needs_setup": name.is_none(),
        // always, before setup (owner, 2026-11-01)
        "setup_key_needed": name.is_none(),
        "version": crate::VERSION,
        "address": base,
        "https": https,
        "domain": app.cfg.domain,
        // no domain yet: a name that points at this address for free (sslip.io), for Caddy's certificate
        "suggested_domain": web::sslip_name(&host),
        "splitter": app.splitter.status_json(),
        "models": app.splitter.models_json(),
    }))
}

#[derive(Deserialize)]
pub struct SetupBody {
    name: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    device_name: Option<String>,
}

/// The setup page's form: the owner's name. Makes the owner, a token for this
/// browser (the owner's page), and the owner's first join link for the apps.
pub async fn setup(State(app): State<Shared>, headers: HeaderMap, Json(body): Json<SetupBody>) -> ApiResult<(StatusCode, Json<Value>)> {
    let name = clean_name(&body.name, MOST_NAME);
    if name.is_empty() {
        return Err(bad("Type your name: your friends will see the server as \"<your name>'s server\"."));
    }
    // the setup key (owner, 2026-11-01: required); none here once it's set up (then it says so below)
    if let Some(key) = &app.cfg.setup_key {
        let typed = body.key.as_deref().unwrap_or("");
        if !crate::auth::same_key(typed, key) {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            return Err(forbidden(if typed.trim().is_empty() { KEY_MISSING } else { KEY_WRONG }));
        }
    }
    let token = auth::new_token();
    let code = auth::new_code();
    let now = now_ms();
    let device_name = clean_name(body.device_name.as_deref().unwrap_or("Web browser"), 60);
    let (person, device) = {
        let mut conn = app.db();
        // one owner: the check and the insert under the same lock
        if let Some((_, owner)) = crate::db::owner(&conn) {
            return Err(conflict(format!("This server is set up already ({}).", crate::server_name(&owner))));
        }
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO people (name, role, created_ms) VALUES (?1, 'owner', ?2)", params![name, now])?;
        let person = tx.last_insert_rowid();
        tx.execute("INSERT INTO devices (person_id, name, token_sha256, created_ms, seen_ms) VALUES (?1, ?2, ?3, ?4, ?4)", params![person, device_name, auth::hash(&token), now])?;
        let device = tx.last_insert_rowid();
        tx.execute("INSERT INTO invites (code, person_id, created_ms) VALUES (?1, ?2, ?3)", params![code, person, now])?;
        tx.commit()?;
        (person, device)
    };
    crate::log(&format!("set up: {}", crate::server_name(&name)));
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": token,
            "person": { "id": person, "name": name, "role": "owner" },
            "device": { "id": device, "name": device_name },
            "server": { "name": crate::server_name(&name) },
            "invite": invite_json(&app, &headers, &code, person, &name, "owner", now),
        })),
    ))
}

/// An invite as the owner sees it: its code, its link and the link's QR.
fn invite_json(app: &Shared, headers: &HeaderMap, code: &str, person: i64, name: &str, role: &str, created_ms: i64) -> Value {
    let link = format!("{}/join/{}", web::base_url(app, headers), auth::show_code(code));
    let expires = created_ms + INVITE_MS;
    json!({
        "code": auth::show_code(code),
        "link": link,
        "qr_svg": web::qr_svg(&link),
        "person": { "id": person, "name": name, "role": role },
        "created_ms": created_ms,
        // unused invites last 7 days (owner, 2026-11-01)
        "expires_ms": expires,
        "expires_in_days": ((expires - now_ms()).max(0) as f64 / 86_400_000.0).ceil() as i64,
    })
}

#[derive(Deserialize)]
pub struct JoinBody {
    code: String,
    #[serde(default)]
    device_name: Option<String>,
}

/// A device joins with an invite's code (the app, after the link or its QR;
/// or a browser, from the join page). The invite is used up.
pub async fn join(State(app): State<Shared>, Json(body): Json<JoinBody>) -> ApiResult<(StatusCode, Json<Value>)> {
    let code = auth::plain_code(&body.code);
    let device_name = clean_name(body.device_name.as_deref().unwrap_or(""), 60);
    let device_name = if device_name.is_empty() { "A device".to_string() } else { device_name };
    let token = auth::new_token();
    let now = now_ms();
    let joined = {
        let mut conn = app.db();
        let found: Option<(i64, Option<i64>, Option<i64>, String, String, Option<i64>, i64)> = conn
            .query_row(
                "SELECT i.person_id, i.used_ms, i.revoked_ms, p.name, p.role, p.revoked_ms, i.created_ms FROM invites i JOIN people p ON p.id = i.person_id WHERE i.code = ?1",
                params![code],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
            )
            .optional()?;
        // an invite past its 7 days is turned away like one taken back
        let expired = found.as_ref().is_some_and(|f| f.1.is_none() && now - f.6 > INVITE_MS);
        let found = found.map(|(a, b, c, d, e, f, _)| (a, b, c, d, e, f));
        match found {
            None => Err(not_found("That invite code isn't right. Check it, or ask the server's owner for a new one.")),
            Some(_) if expired => Err(ApiError(StatusCode::GONE, "That invite expired: invites last 7 days. Ask the server's owner for a new one.".into())),
            Some((_, Some(_), ..)) => Err(ApiError(StatusCode::GONE, "That invite was used already: each one works once. Ask the server's owner for a new one.".into())),
            Some((_, _, Some(_), ..)) | Some((.., Some(_))) => Err(ApiError(StatusCode::GONE, "The owner took that invite back. Ask them for a new one.".into())),
            Some((person, None, None, name, role, None)) => {
                let tx = conn.transaction()?;
                tx.execute("INSERT INTO devices (person_id, name, token_sha256, created_ms, seen_ms) VALUES (?1, ?2, ?3, ?4, ?4)", params![person, device_name, auth::hash(&token), now])?;
                let device = tx.last_insert_rowid();
                tx.execute("UPDATE invites SET used_ms = ?1, device_id = ?2 WHERE code = ?3", params![now, device, code])?;
                tx.commit()?;
                Ok((person, name, role, device))
            }
        }
    };
    let (person, name, role, device) = match joined {
        Ok(j) => j,
        Err(e) => {
            // a wrong guess waits a little (codes can't be tried quickly)
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            return Err(e);
        }
    };
    crate::log(&format!("{name} joined with a device ({device_name})"));
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": token,
            "person": { "id": person, "name": name, "role": role },
            "device": { "id": device, "name": device_name },
            "server": { "name": app.server_name() },
        })),
    ))
}

/// Who this device is.
pub async fn me(State(app): State<Shared>, who: Who) -> Json<Value> {
    let device_name: String = app.db().query_row("SELECT name FROM devices WHERE id = ?1", params![who.device], |r| r.get(0)).unwrap_or_default();
    Json(json!({
        "person": { "id": who.person, "name": who.name, "role": if who.owner { "owner" } else { "friend" } },
        "device": { "id": who.device, "name": device_name },
        "server": { "name": app.server_name() },
        // what the owner lets this person do (the owner: everything)
        "can": { "add": who.can_add, "split": who.can_split, "host": who.can_host },
    }))
}

#[derive(Deserialize)]
pub struct InviteBody {
    /// A new friend's name.
    #[serde(default)]
    name: Option<String>,
    /// Or someone already here (the owner's own next device, a friend's second phone).
    #[serde(default)]
    person: Option<i64>,
}

/// The owner makes an invite: for a new friend (`name`), or another device of
/// someone already here (`person`).
pub async fn invite(State(app): State<Shared>, Owner(_): Owner, headers: HeaderMap, Json(body): Json<InviteBody>) -> ApiResult<(StatusCode, Json<Value>)> {
    let now = now_ms();
    let code = auth::new_code();
    let (person, name, role) = {
        let mut conn = app.db();
        let tx = conn.transaction()?;
        let who = match (body.person, body.name.as_deref().map(|n| clean_name(n, MOST_NAME))) {
            (Some(id), _) => {
                let found: Option<(String, String, Option<i64>)> = tx.query_row("SELECT name, role, revoked_ms FROM people WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
                match found {
                    None => return Err(not_found("There's nobody like that on this server.")),
                    Some((_, _, Some(_))) => return Err(conflict("That friend was removed. Invite them again by name.")),
                    Some((name, role, None)) => (id, name, role),
                }
            }
            (None, Some(name)) if !name.is_empty() => {
                tx.execute("INSERT INTO people (name, role, created_ms) VALUES (?1, 'friend', ?2)", params![name, now])?;
                (tx.last_insert_rowid(), name, "friend".to_string())
            }
            _ => return Err(bad("Type your friend's name (so you know whose invite it is).")),
        };
        tx.execute("INSERT INTO invites (code, person_id, created_ms) VALUES (?1, ?2, ?3)", params![code, who.0, now])?;
        tx.commit()?;
        who
    };
    Ok((StatusCode::CREATED, Json(invite_json(&app, &headers, &code, person, &name, &role, now))))
}

/// The invites not used yet (each can be shown again, or taken back).
pub async fn invites(State(app): State<Shared>, Owner(_): Owner, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, i64, String, String, i64)> = {
        let conn = app.db();
        let mut st = conn.prepare(
            "SELECT i.code, p.id, p.name, p.role, i.created_ms FROM invites i JOIN people p ON p.id = i.person_id
             WHERE i.used_ms IS NULL AND i.revoked_ms IS NULL AND p.revoked_ms IS NULL AND i.created_ms > ?1 ORDER BY i.created_ms DESC",
        )?;
        let rows = st.query_map(params![now_ms() - INVITE_MS], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
        rows
    };
    let list: Vec<Value> = rows.iter().map(|(code, id, name, role, at)| invite_json(&app, &headers, code, *id, name, role, *at)).collect();
    Ok(Json(json!({ "invites": list })))
}

/// The owner takes back an invite nobody has used yet.
pub async fn take_back(State(app): State<Shared>, Owner(_): Owner, Path(code): Path<String>) -> ApiResult<Json<Value>> {
    let code = auth::plain_code(&code);
    let conn = app.db();
    let used: Option<Option<i64>> = conn.query_row("SELECT used_ms FROM invites WHERE code = ?1", params![code], |r| r.get(0)).optional()?;
    match used {
        None => Err(not_found("There's no invite with that code.")),
        Some(Some(_)) => Err(conflict("That invite was used already: remove the device it made instead.")),
        Some(None) => {
            conn.execute("UPDATE invites SET revoked_ms = ?1 WHERE code = ?2 AND revoked_ms IS NULL", params![now_ms(), code])?;
            Ok(Json(json!({ "ok": true })))
        }
    }
}

/// Everyone on the server, with their devices (the owner's page).
pub async fn people(State(app): State<Shared>, Owner(_): Owner) -> ApiResult<Json<Value>> {
    let conn = app.db();
    let mut st = conn.prepare("SELECT id, name, role, created_ms, revoked_ms, can_add, can_split, can_host FROM people ORDER BY role = 'owner' DESC, revoked_ms IS NOT NULL, name COLLATE NOCASE")?;
    #[allow(clippy::type_complexity)]
    let people: Vec<(i64, String, String, i64, Option<i64>, bool, bool, bool)> =
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)))?.collect::<Result<_, _>>()?;
    let mut dev = conn.prepare("SELECT id, name, created_ms, seen_ms, revoked_ms FROM devices WHERE person_id = ?1 ORDER BY created_ms")?;
    let mut open = conn.prepare("SELECT COUNT(*), COALESCE(SUM(created_ms <= ?2), 0) FROM invites WHERE person_id = ?1 AND used_ms IS NULL AND revoked_ms IS NULL")?;
    let since = now_ms() - INVITE_MS;
    let mut list = vec![];
    for (id, name, role, created, revoked, can_add, can_split, can_host) in people {
        let devices: Vec<Value> = dev
            .query_map(params![id], |r| {
                Ok(json!({ "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "created_ms": r.get::<_, i64>(2)?, "seen_ms": r.get::<_, Option<i64>>(3)?, "removed": r.get::<_, Option<i64>>(4)?.is_some() }))
            })?
            .collect::<Result<_, _>>()?;
        let (unused, expired): (i64, i64) = open.query_row(params![id, since], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let owner = role == "owner";
        list.push(json!({
            "id": id, "name": name, "role": role, "created_ms": created, "removed": revoked.is_some(), "devices": devices,
            "open_invites": unused - expired, "expired_invites": expired,
            "can": { "add": owner || can_add, "split": owner || can_split, "host": owner || can_host },
        }));
    }
    Ok(Json(json!({ "people": list })))
}

#[derive(Deserialize)]
pub struct SwitchesBody {
    #[serde(default)]
    can_add: Option<bool>,
    #[serde(default)]
    can_split: Option<bool>,
    #[serde(default)]
    can_host: Option<bool>,
}

/// PATCH /api/people/ID {can_add, can_split, can_host}: the owner's switches
/// for a friend (all on at first; owner, 2026-11-01).
pub async fn switches(State(app): State<Shared>, Owner(_): Owner, Path(id): Path<i64>, Json(body): Json<SwitchesBody>) -> ApiResult<Json<Value>> {
    let conn = app.db();
    let found: Option<(String, Option<i64>)> = conn.query_row("SELECT role, revoked_ms FROM people WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    match found {
        None => return Err(not_found("There's nobody like that on this server.")),
        Some((role, _)) if role == "owner" => return Err(forbidden("It's your server: you can always do everything on it.")),
        Some((_, Some(_))) => return Err(conflict("That friend was removed.")),
        _ => {}
    }
    for (column, value) in [("can_add", body.can_add), ("can_split", body.can_split), ("can_host", body.can_host)] {
        if let Some(v) = value {
            conn.execute(&format!("UPDATE people SET {column} = ?1 WHERE id = ?2"), params![v, id])?;
        }
    }
    let (name, add, split, host): (String, bool, bool, bool) = conn.query_row("SELECT name, can_add, can_split, can_host FROM people WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(Json(json!({ "id": id, "name": name, "can": { "add": add, "split": split, "host": host } })))
}

/// The owner removes a friend: their devices stop working at once and their
/// open invites are taken back. The songs they added stay in the library.
pub async fn revoke_person(State(app): State<Shared>, Owner(_): Owner, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    {
        let mut conn = app.db();
        let role: Option<String> = conn.query_row("SELECT role FROM people WHERE id = ?1", params![id], |r| r.get(0)).optional()?;
        match role.as_deref() {
            None => return Err(not_found("There's nobody like that on this server.")),
            Some("owner") => return Err(forbidden("You can't remove yourself: it's your server. Remove one of your devices instead.")),
            _ => {}
        }
        let now = now_ms();
        let tx = conn.transaction()?;
        tx.execute("UPDATE people SET revoked_ms = ?1 WHERE id = ?2 AND revoked_ms IS NULL", params![now, id])?;
        tx.execute("UPDATE devices SET revoked_ms = ?1 WHERE person_id = ?2 AND revoked_ms IS NULL", params![now, id])?;
        tx.execute("UPDATE invites SET revoked_ms = ?1 WHERE person_id = ?2 AND used_ms IS NULL AND revoked_ms IS NULL", params![now, id])?;
        tx.commit()?;
    }
    // out of the party too
    app.party.recheck_listeners(&app);
    Ok(Json(json!({ "ok": true })))
}

/// A device stops working: the owner removes any; a friend, their own (a lost phone).
pub async fn revoke_device(State(app): State<Shared>, who: Who, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    {
        let conn = app.db();
        let found: Option<(i64, String)> = conn.query_row("SELECT d.person_id, p.role FROM devices d JOIN people p ON p.id = d.person_id WHERE d.id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        let Some((person, role)) = found else { return Err(not_found("There's no such device.")) };
        if !who.owner && person != who.person {
            return Err(forbidden("You can only remove your own devices."));
        }
        if role == "owner" {
            let left: i64 = conn.query_row("SELECT COUNT(*) FROM devices WHERE person_id = ?1 AND revoked_ms IS NULL AND id != ?2", params![person, id], |r| r.get(0))?;
            if left == 0 {
                return Err(conflict("That's your last device on the server: keep it, so you can still manage the server."));
            }
        }
        conn.execute("UPDATE devices SET revoked_ms = ?1 WHERE id = ?2 AND revoked_ms IS NULL", params![now_ms(), id])?;
    }
    app.party.recheck_listeners(&app);
    Ok(Json(json!({ "ok": true })))
}
