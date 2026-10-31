//! The server's own web pages: setup and the owner's page (/), a join page
//! for each invite (/join/CODE), and the listening party in a browser
//! (/party). Plain HTML and a little JavaScript, in the app's dark look
//! (Stone or True Black, the orange accent; never navy).

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use rusqlite::{params, OptionalExtension};

use crate::{auth, Shared};

const INDEX: &str = include_str!("../web/index.html");
const JOIN: &str = include_str!("../web/join.html");
const PARTY: &str = include_str!("../web/party.html");
const CSS: &str = include_str!("../web/app.css");
const JS: &str = include_str!("../web/app.js");
const LOGO: &str = include_str!("../web/logo.svg");

/// The address links start with: RMS_PUBLIC_URL, else https://RMS_DOMAIN, else
/// as the browser reached it (Caddy says whether that was https).
pub fn base_url(app: &Shared, headers: &HeaderMap) -> String {
    if let Some(u) = &app.cfg.public_url {
        return u.clone();
    }
    if let Some(d) = &app.cfg.domain {
        return format!("https://{d}");
    }
    let h = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).map(|v| v.split(',').next().unwrap_or("").trim().to_string()).filter(|v| !v.is_empty());
    let proto = h("x-forwarded-proto").filter(|p| p == "https" || p == "http").unwrap_or_else(|| "http".into());
    let host = h("x-forwarded-host").or_else(|| h("host")).unwrap_or_else(|| "localhost".into());
    // only what a host name can hold (it goes into links and pages)
    let host: String = host.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']')).collect();
    format!("{proto}://{host}")
}

/// No domain yet: "203-0-113-5.sslip.io" points at 203.0.113.5 for free, so
/// Caddy can still get a certificate (only for a public IPv4 address).
pub fn sslip_name(host: &str) -> Option<String> {
    let ip = host.split(':').next()?;
    let parts: Vec<u8> = ip.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let ip = std::net::Ipv4Addr::new(*parts.first()?, *parts.get(1)?, *parts.get(2)?, *parts.get(3)?);
    if parts.len() != 4 || ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
        return None;
    }
    Some(format!("{}.sslip.io", ip.to_string().replace('.', "-")))
}

/// A link's QR code as SVG (dark on white, as scanners like it), ready to put in a page.
pub fn qr_svg(text: &str) -> String {
    use qrcode::render::svg;
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else { return String::new() };
    let svg = code.render::<svg::Color>().min_dimensions(232, 232).quiet_zone(true).dark_color(svg::Color("#0C0A09")).light_color(svg::Color("#FFFFFF")).build();
    // just the <svg> (no XML header), so it can sit inside a page
    match svg.find("<svg") {
        Some(i) => svg[i..].to_string(),
        None => svg,
    }
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn page(html: &str) -> Response {
    ([(header::CACHE_CONTROL, "no-cache"), (header::CONTENT_TYPE, "text/html; charset=utf-8")], html.to_string()).into_response()
}

pub async fn home() -> Response {
    page(INDEX)
}

pub async fn party_page() -> Response {
    page(PARTY)
}

/// An invite's page: its code and QR for the apps, and a way to listen in this browser.
pub async fn join_page(State(app): State<Shared>, headers: HeaderMap, Path(code): Path<String>) -> Response {
    let plain = auth::plain_code(&code);
    let found: Option<(String, Option<i64>, Option<i64>, Option<i64>, i64)> = app
        .db()
        .query_row("SELECT p.name, i.used_ms, i.revoked_ms, p.revoked_ms, i.created_ms FROM invites i JOIN people p ON p.id = i.person_id WHERE i.code = ?1", params![plain], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .optional()
        .ok()
        .flatten();
    let server = app.server_name().unwrap_or_else(|| "Radiance Music Server".into());
    let link = format!("{}/join/{}", base_url(&app, &headers), auth::show_code(&plain));
    let (state, who, problem) = match &found {
        None => ("bad", String::new(), "That invite code isn't right. Check the link, or ask the server's owner for a new one."),
        Some((_, Some(_), ..)) => ("used", String::new(), "This invite was used already: each one works once. Ask the server's owner for a new one."),
        Some((_, _, Some(_), ..)) | Some((_, _, _, Some(_), _)) => ("gone", String::new(), "The owner took this invite back. Ask them for a new one."),
        Some((.., created)) if crate::now_ms() - created > crate::db::INVITE_MS => ("expired", String::new(), "This invite expired: invites last 7 days. Ask the server's owner for a new one."),
        Some((name, None, None, None, _)) => ("ok", name.clone(), ""),
    };
    let html = JOIN
        .replace("{{SERVER}}", &escape(&server))
        .replace("{{STATE}}", state)
        .replace("{{WHO}}", &escape(&who))
        .replace("{{PROBLEM}}", &escape(problem))
        .replace("{{CODE}}", &escape(&auth::show_code(&plain)))
        .replace("{{LINK}}", &escape(&link))
        .replace("{{QR}}", &if state == "ok" { qr_svg(&link) } else { String::new() });
    let status = if state == "ok" { StatusCode::OK } else { StatusCode::NOT_FOUND };
    (status, page(&html)).into_response()
}

pub async fn asset(Path(name): Path<String>) -> Response {
    let (kind, body) = match name.as_str() {
        "app.css" => ("text/css; charset=utf-8", CSS),
        "app.js" => ("text/javascript; charset=utf-8", JS),
        "logo.svg" => ("image/svg+xml", LOGO),
        _ => return (StatusCode::NOT_FOUND, "no such file").into_response(),
    };
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}
