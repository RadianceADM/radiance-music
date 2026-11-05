//! HTTP, run on worker threads only.

use std::sync::OnceLock;
use std::time::Duration;

use serde_json::Value;

pub const USER_AGENT: &str = concat!("Radiance Music/", env!("CARGO_PKG_VERSION"), " (https://github.com/RadianceADM/radiance-music)");

pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        // IPv4 only: on a network whose IPv6 is broken every new connection would
        // first wait for IPv6 to time out (7-9 s). yt-dlp uses IPv4 too (-4), and
        // must: YouTube ties a stream's address to the IP that asked for it.
        ureq::Agent::config_builder()
            .ip_family(ureq::config::IpFamily::Ipv4Only)
            .user_agent(USER_AGENT)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(20)))
            .build()
            .into()
    })
}

pub fn get_json(url: &str, query: &[(&str, &str)]) -> Option<Value> {
    let mut req = agent().get(url);
    for (k, v) in query {
        req = req.query(*k, *v);
    }
    req.call().ok()?.body_mut().read_json().ok()
}

pub fn post_json(url: &str, body: &Value, headers: &[(&str, &str)]) -> Option<Value> {
    let mut req = agent().post(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req.send_json(body).ok()?.body_mut().read_json().ok()
}

/// A whole small file (album art), up to 8 MB.
pub fn get_bytes(url: &str) -> Option<Vec<u8>> {
    agent().get(url).call().ok()?.body_mut().with_config().limit(8 << 20).read_to_vec().ok()
}

/// The internet answers: YouTube Music's name resolves and its server takes a connection
/// (3 s at most; worker thread). GIO's network monitor can say "online" when it isn't (it
/// asks NetworkManager, which can't see a broken DNS or a sign-in-first Wi-Fi), so an
/// empty search or a song that won't load asks this too.
pub fn reachable() -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let Ok(mut addrs) = ("music.youtube.com", 443).to_socket_addrs() else { return false };
    addrs.next().is_some_and(|a| TcpStream::connect_timeout(&a, Duration::from_secs(3)).is_ok())
}
