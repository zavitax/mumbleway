//! Importing server definitions from links and profile files.
//!
//! Two formats are supported:
//!
//! * **`mumble://` links** — the scheme the official client registers, and what
//!   community sites and invite links hand out:
//!   `mumble://user:password@host:port/Channel/Sub?title=Name&version=1.2.0`
//! * **JSON profile files** — either a single object or an array, so a list of
//!   servers can be shared as one file.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::net::{ProxyKind, ProxySpec};
use crate::session::ServerProfile;

/// Mumble's default port, used when a link omits one.
pub const DEFAULT_PORT: u16 = 64738;

/// Where a shareable invitation points.
///
/// **An https link exists because `mumble://` does not survive a messaging
/// app.** Measured on a device rather than assumed: Telegram is inconsistent
/// about linkifying a `mumble://` URL at all, and when it does, tapping it
/// opens its in-app Chrome Custom Tab, which tries to load the scheme as a web
/// address and fails. Every messenger linkifies https, and Android's App Links
/// hand a verified https URL to the app instead of to a browser.
///
/// The `mumble://` form is not going anywhere: it is what the official client
/// registers, what a phone's camera app offers for a scanned QR code, and what
/// works with no domain and no network.
pub const WEB_INVITE_BASE: &str = "https://zavitax.github.io/mumbleway/join/";

/// The scheme a proxy on its own is shared under.
///
/// **A proxy is worth sharing by itself.** On a network where nothing gets out
/// directly, the first thing a new rider needs is the way out — before any
/// server is worth adding. It travels like a server does: as a link, as a QR
/// code, and wrapped in the same https page for messengers that will not carry
/// a private scheme.
pub const PROXY_SCHEME: &str = "mumble-proxy";

/// Host and path of [`WEB_INVITE_BASE`], for recognising one on the way back in.
const WEB_INVITE_HOST: &str = "zavitax.github.io";
const WEB_INVITE_PATH: &str = "/mumbleway/join";

/// On-disk shape of a profile file. Deliberately forgiving: everything except
/// the host has a sensible default, so a minimal hand-written file works.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileFileEntry {
    pub host: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub cert_fingerprint: Option<String>,
    /// A proxy to reach this server through, in the same form a link uses:
    /// `socks5://host:port`, optionally with credentials and `?voice=1`.
    ///
    /// Written as text rather than as a nested object so that a file and a
    /// link say the same thing in the same words — one parser, one shape, and
    /// a rider can move a proxy between the two by copying it.
    #[serde(default)]
    pub proxy: Option<String>,
}

impl ProfileFileEntry {
    fn into_profile(self, fallback_username: &str) -> ServerProfile {
        let port = self.port.unwrap_or(DEFAULT_PORT);
        let name = self.name.unwrap_or_else(|| self.host.clone());
        let username = self
            .username
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| fallback_username.to_string());

        let mut p = ServerProfile::new(name, self.host, port, username);
        p.password = self.password.filter(|s| !s.is_empty());
        p.auto_join_channel = self.channel.filter(|s| !s.is_empty());
        p.cert_fingerprint = self.cert_fingerprint;
        p.proxy_chain = self
            .proxy
            .as_deref()
            .and_then(parse_proxy)
            .into_iter()
            .collect();
        p
    }
}

/// Parses a `mumble://` link.
///
/// `fallback_username` is used when the link carries none, which is the common
/// case for public invite links.
pub fn parse_url(input: &str, fallback_username: &str) -> Result<ServerProfile> {
    let trimmed = input.trim();
    let parsed =
        url::Url::parse(trimmed).map_err(|e| CoreError::Other(format!("not a valid link: {e}")))?;

    // An invitation shared as an https link carries the real one in its
    // fragment, so unwrap that and carry on as if it had arrived directly.
    //
    // **The fragment is the point, not a detail.** A browser never sends it to
    // the server, so the host, the channel and any password stay on the device
    // even though the link was fetched over the network. Putting them in the
    // path or the query would write every invitation into a GitHub access log.
    if let Some(inner) = web_invite_payload(&parsed) {
        return parse_url(&inner, fallback_username);
    }

    if !parsed.scheme().eq_ignore_ascii_case("mumble") {
        return Err(CoreError::Other(format!(
            "expected a mumble:// link, got {}://",
            parsed.scheme()
        )));
    }

    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| CoreError::Other("link has no server address".into()))?
        .to_string();

    let port = parsed.port().unwrap_or(DEFAULT_PORT);

    // Links often carry no username; fall back rather than rejecting them.
    let username = {
        let u = percent_decode(parsed.username());
        if u.trim().is_empty() {
            fallback_username.to_string()
        } else {
            u
        }
    };

    let password = parsed
        .password()
        .map(percent_decode)
        .filter(|p| !p.is_empty());

    // The path is a channel path; take the last segment as the channel to join.
    let channel = parsed
        .path_segments()
        .and_then(|segs| {
            segs.filter(|s| !s.is_empty())
                .map(percent_decode)
                .next_back()
        })
        .filter(|s| !s.is_empty());

    // `title` is what the official client uses for the display name.
    let title = parsed
        .query_pairs()
        .find(|(k, _)| k == "title")
        .map(|(_, v)| v.to_string())
        .filter(|t| !t.trim().is_empty());

    // A proxy the sharer named for this server, if the link carries one.
    //
    // **Only an explicitly chosen one ever travels**, which the sharing side
    // decides; here it is simply read. What arrives is shown before it is used
    // — adopting somebody else's route silently is the thing to avoid, not
    // carrying it, and a server that can only be reached through a proxy is
    // not much of an invitation without one.
    let proxy = parsed
        .query_pairs()
        .find(|(k, _)| k == "proxy")
        .and_then(|(_, v)| parse_proxy(&v));

    let mut profile =
        ServerProfile::new(title.unwrap_or_else(|| host.clone()), host, port, username);
    profile.password = password;
    profile.auto_join_channel = channel;
    profile.proxy_chain = proxy.into_iter().collect();
    Ok(profile)
}

/// `socks5://[user:password@]host:port[?voice=1]`, as a link carries it.
///
/// Returns `None` for anything it cannot read in full: half a proxy is a route
/// nobody chose, and refusing it leaves the server reachable directly, which is
/// the safer of the two failures.
fn parse_proxy(text: &str) -> Option<ProxySpec> {
    let text = percent_decode(text);
    let (scheme, rest) = text.split_once("://")?;
    let kind = match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => ProxyKind::HttpConnect,
        "socks" | "socks5" => ProxyKind::Socks5,
        _ => return None,
    };

    let (rest, tunnel_voice) = match rest.split_once('?') {
        Some((head, query)) => (head, query.split('&').any(|p| p == "voice=1")),
        None => (rest, false),
    };

    let (credentials, address) = match rest.rsplit_once('@') {
        Some((c, a)) => (Some(c), a),
        None => (None, rest),
    };
    let (username, password) = match credentials {
        Some(c) => match c.split_once(':') {
            Some((u, p)) => (
                Some(percent_decode(u)).filter(|u| !u.is_empty()),
                Some(percent_decode(p)).filter(|p| !p.is_empty()),
            ),
            None => (Some(percent_decode(c)).filter(|u| !u.is_empty()), None),
        },
        None => (None, None),
    };

    // An IPv6 literal is bracketed; anything else splits at the last colon.
    let (host, port) = if let Some(close) = address.find(']') {
        let host = address.get(1..close)?;
        let port = address.get(close + 2..)?;
        (host, port)
    } else {
        address.rsplit_once(':')?
    };
    let port: u16 = port.parse().ok()?;
    if host.trim().is_empty() || port == 0 {
        return None;
    }

    Some(ProxySpec {
        kind,
        host: host.to_string(),
        port,
        username,
        password,
        tunnel_voice,
    })
}

/// A proxy as text: `socks5://host:port`, optionally with credentials and
/// `?voice=1`.
///
/// **One form everywhere it is written down** — in a server link's `proxy`
/// parameter, in a profile file, and inside a `mumble-proxy://` link — so a
/// rider can move one between them by copying, and so there is one parser to
/// be wrong about.
pub fn build_proxy_text(spec: &ProxySpec, include_credentials: bool) -> String {
    build_proxy(spec, include_credentials)
}

/// Reads what [`build_proxy_text`] writes.
pub fn parse_proxy_text(text: &str) -> Option<ProxySpec> {
    parse_proxy(text)
}

fn build_proxy(spec: &ProxySpec, include_credentials: bool) -> String {
    let mut out = String::from(match spec.kind {
        ProxyKind::HttpConnect => "http://",
        ProxyKind::Socks5 => "socks5://",
    });
    if include_credentials {
        if let Some(u) = spec.username.as_ref().filter(|u| !u.is_empty()) {
            out.push_str(&percent_encode(u));
            if let Some(p) = spec.password.as_ref().filter(|p| !p.is_empty()) {
                out.push(':');
                out.push_str(&percent_encode(p));
            }
            out.push('@');
        }
    }
    if spec.host.contains(':') {
        out.push_str(&format!("[{}]", spec.host));
    } else {
        out.push_str(&spec.host);
    }
    out.push_str(&format!(":{}", spec.port));
    if spec.tunnel_voice {
        out.push_str("?voice=1");
    }
    out
}

/// The `mumble://` link inside a web invitation, if `parsed` is one.
///
/// Deliberately strict about host and path: this is what decides whether a
/// fragment is treated as a link to follow, and the whole tree of links from
/// somebody else's site should not be.
fn web_invite_payload(parsed: &url::Url) -> Option<String> {
    if !matches!(parsed.scheme(), "https" | "http") {
        return None;
    }
    if !parsed.host_str()?.eq_ignore_ascii_case(WEB_INVITE_HOST) {
        return None;
    }
    if !parsed.path().starts_with(WEB_INVITE_PATH) {
        return None;
    }
    let fragment = parsed.fragment()?.trim();
    // Percent-decoded because a messenger, a QR reader or a browser may hand it
    // back encoded, and `mumble://` survives either way.
    let decoded = percent_decode(fragment);
    let candidate = if decoded.trim().is_empty() {
        fragment.to_string()
    } else {
        decoded
    };
    // Either scheme: the page carries both, and tells them apart by exactly
    // this prefix before it decides which button to show.
    let lower = candidate.to_ascii_lowercase();
    (lower.starts_with("mumble:") || lower.starts_with("mumble-proxy:")).then_some(candidate)
}

fn percent_decode(s: &str) -> String {
    percent_decode_bytes(s.as_bytes())
}

/// Minimal percent-decoder. Usernames and channel names routinely contain
/// spaces and accented characters, which links encode.
fn percent_decode_bytes(bytes: &[u8]) -> String {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses a JSON profile file containing either one server or an array.
pub fn parse_json(input: &str, fallback_username: &str) -> Result<Vec<ServerProfile>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CoreError::Other("the file is empty".into()));
    }

    let entries: Vec<ProfileFileEntry> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)
            .map_err(|e| CoreError::Other(format!("could not read profile list: {e}")))?
    } else {
        let one: ProfileFileEntry = serde_json::from_str(trimmed)
            .map_err(|e| CoreError::Other(format!("could not read profile: {e}")))?;
        vec![one]
    };

    let profiles: Vec<ServerProfile> = entries
        .into_iter()
        .filter(|e| !e.host.trim().is_empty())
        .map(|e| e.into_profile(fallback_username))
        .collect();

    if profiles.is_empty() {
        return Err(CoreError::Other(
            "the file contained no servers with an address".into(),
        ));
    }
    Ok(profiles)
}

/// Percent-encodes a single URL component.
///
/// Deliberately conservative: everything outside the unreserved set is escaped,
/// so usernames and channel names containing spaces, accents, `@`, `:` or `/`
/// survive a round trip through [`parse_url`].
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Builds a `mumble://` invite link for sharing.
///
/// `include_password` is a real decision, not a convenience flag: a link with a
/// password in it grants access to anyone who ever sees it, including whatever
/// chat app it passes through. The caller must choose deliberately.
pub fn build_url(profile: &ServerProfile, channel: Option<&str>, include_password: bool) -> String {
    let mut url = String::from("mumble://");

    // Deliberately no username.
    //
    // An invitation says where to go, not who is going. The name belonged to
    // whoever generated the code, and every rider who scanned it arrived under
    // that same name — which a Mumble server answers by refusing the second
    // connection or by quietly appending a digit. Neither is what the person
    // sharing the code meant to hand out. The scanning device supplies its own;
    // see DeviceIdentity on the Dart side.
    //
    // The password stays, and rides in the userinfo field with the username
    // left empty — `mumble://:secret@host`, which is ordinary URI syntax. It is
    // the *server's* password, the same one for everybody, so unlike the name
    // it is genuinely part of the invitation.
    if include_password {
        if let Some(p) = profile.password.as_ref().filter(|p| !p.is_empty()) {
            url.push(':');
            url.push_str(&percent_encode(p));
            url.push('@');
        }
    }

    url.push_str(&profile.host);
    if profile.port != DEFAULT_PORT {
        url.push_str(&format!(":{}", profile.port));
    }

    url.push('/');
    if let Some(c) = channel.filter(|c| !c.trim().is_empty()) {
        url.push_str(&percent_encode(c));
    }

    let mut query: Vec<String> = Vec::new();
    if !profile.name.trim().is_empty() && profile.name != profile.host {
        query.push(format!("title={}", percent_encode(&profile.name)));
    }
    // **The route travels with the invitation.** A server that can only be
    // reached through a proxy is not much of an invitation without it. Only
    // the first hop: the rest of a chain is the sharer's own arrangement for
    // getting out of their network, which says nothing about reaching this
    // server from anywhere else.
    if let Some(proxy) = profile.proxy_chain.last() {
        query.push(format!(
            "proxy={}",
            percent_encode(&build_proxy(proxy, include_password))
        ));
    }
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query.join("&"));
    }
    url
}

/// Builds the shareable https invitation: [`WEB_INVITE_BASE`] with the
/// `mumble://` link in the fragment.
///
/// The fragment is written verbatim rather than percent-encoded. Every
/// character [`build_url`] emits is legal in a fragment, and a link somebody
/// might read out or paste by hand is worth keeping readable; the parser
/// accepts either form on the way back.
pub fn build_web_url(
    profile: &ServerProfile,
    channel: Option<&str>,
    include_password: bool,
) -> String {
    format!(
        "{WEB_INVITE_BASE}#{}",
        build_url(profile, channel, include_password)
    )
}

/// Builds a JSON profile file for sharing, optionally with the password.
pub fn build_json(
    profile: &ServerProfile,
    channel: Option<&str>,
    include_password: bool,
) -> Result<String> {
    let entry = ProfileFileEntry {
        host: profile.host.clone(),
        name: Some(profile.name.clone()),
        port: Some(profile.port),
        // None, for the reason given in build_url: a name identifies the rider
        // holding the device, and is the one field in a profile that must not
        // travel with it.
        username: None,
        password: if include_password {
            profile.password.clone()
        } else {
            None
        },
        channel: channel.map(|c| c.to_string()),
        // Never shared: the pin is this device's own trust decision, and
        // copying it would launder it onto someone else's device.
        cert_fingerprint: None,
        // Shared, unlike the pin, and for the opposite reason: a route is
        // about reaching the server rather than about trusting it, and a
        // server that needs one is unusable without it. Credentials ride only
        // when the password does — the rider was asked about exactly that.
        proxy: profile
            .proxy_chain
            .last()
            .map(|p| build_proxy(p, include_password)),
    };
    serde_json::to_string_pretty(&vec![entry])
        .map_err(|e| CoreError::Other(format!("could not build profile: {e}")))
}

/// Builds a `mumble-proxy://` link for one proxy.
///
/// Credentials only when asked for, the same question a server's password
/// gets: a proxy login in a link is usable by whoever receives it, for
/// anything.
pub fn build_proxy_url(spec: &ProxySpec, include_credentials: bool) -> String {
    let inner = build_proxy(spec, include_credentials);
    // `build_proxy` writes `http://host:port`; the kind is carried as a
    // parameter here instead, so the scheme can stay `mumble-proxy` and be
    // recognised by a phone before anything is parsed.
    let (kind, rest) = inner.split_once("://").unwrap_or(("http", inner.as_str()));
    let (address, voice) = match rest.split_once('?') {
        Some((a, q)) => (a, q.contains("voice=1")),
        None => (rest, false),
    };
    format!(
        "{PROXY_SCHEME}://{address}/?kind={kind}{}",
        if voice { "&voice=1" } else { "" }
    )
}

/// The same, as the https wrapper that survives a messenger.
pub fn build_proxy_web_url(spec: &ProxySpec, include_credentials: bool) -> String {
    format!(
        "{WEB_INVITE_BASE}#{}",
        build_proxy_url(spec, include_credentials)
    )
}

/// Reads a `mumble-proxy://` link, or the https wrapper around one.
///
/// Returns `None` for anything else, including a `mumble://` server link: the
/// two look alike and mean entirely different things, and a page or a handler
/// that confuses them would add a server as a proxy or the reverse.
pub fn parse_proxy_url(input: &str) -> Option<ProxySpec> {
    let trimmed = input.trim();
    let text = if let Ok(parsed) = url::Url::parse(trimmed) {
        match web_invite_payload(&parsed) {
            Some(inner) => inner,
            None => trimmed.to_string(),
        }
    } else {
        trimmed.to_string()
    };

    let rest = text
        .strip_prefix(&format!("{PROXY_SCHEME}://"))
        .or_else(|| text.strip_prefix(&format!("{PROXY_SCHEME}:")))?;

    // Everything after the address is a query; the kind lives there so that the
    // scheme itself stays one word a platform can register.
    let (address, query) = match rest.split_once('?') {
        Some((a, q)) => (a.trim_end_matches('/'), q),
        None => (rest.trim_end_matches('/'), ""),
    };
    let kind = query
        .split('&')
        .find_map(|p| p.strip_prefix("kind="))
        .unwrap_or("http");
    let voice = query.split('&').any(|p| p == "voice=1");

    parse_proxy(&format!(
        "{kind}://{address}{}",
        if voice { "?voice=1" } else { "" }
    ))
}

/// Accepts either a link or a JSON file body and returns whatever it finds.
pub fn parse_any(input: &str, fallback_username: &str) -> Result<Vec<ServerProfile>> {
    let trimmed = input.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        parse_json(trimmed, fallback_username)
    } else {
        parse_url(trimmed, fallback_username).map(|p| vec![p])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_link() {
        let p = parse_url(
            "mumble://alice:secret@voice.example.com:64744/Lobby/Riders?title=Sunday%20Ride",
            "fallback",
        )
        .unwrap();

        assert_eq!(p.host, "voice.example.com");
        assert_eq!(p.port, 64744);
        assert_eq!(p.username, "alice");
        assert_eq!(p.password.as_deref(), Some("secret"));
        assert_eq!(p.auto_join_channel.as_deref(), Some("Riders"));
        assert_eq!(p.name, "Sunday Ride");
    }

    #[test]
    fn defaults_the_port_and_username() {
        // The common invite-link shape: host only.
        let p = parse_url("mumble://mumble.example.com", "rider").unwrap();
        assert_eq!(p.port, DEFAULT_PORT);
        assert_eq!(p.username, "rider", "must fall back, not reject");
        assert_eq!(p.password, None);
        assert_eq!(p.auto_join_channel, None);
        assert_eq!(p.name, "mumble.example.com");
    }

    #[test]
    fn decodes_percent_escapes() {
        let p = parse_url("mumble://two%20words@host/Caf%C3%A9", "x").unwrap();
        assert_eq!(p.username, "two words");
        assert_eq!(p.auto_join_channel.as_deref(), Some("Café"));
    }

    #[test]
    fn rejects_other_schemes_and_junk() {
        assert!(parse_url("https://example.com", "u").is_err());
        assert!(parse_url("not a url", "u").is_err());
        assert!(parse_url("", "u").is_err());
        // A scheme with no host is useless.
        assert!(parse_url("mumble://", "u").is_err());
    }

    #[test]
    fn ids_match_what_the_manager_expects() {
        // The id must be host:port so an imported server collides with an
        // existing entry for the same server rather than duplicating it.
        let p = parse_url("mumble://host.example:64738/", "u").unwrap();
        assert_eq!(p.id, "host.example:64738");
    }

    #[test]
    fn parses_a_single_json_profile() {
        let json = r#"{"host":"a.example","name":"Alpha","port":1234,"username":"bob"}"#;
        let v = parse_json(json, "fallback").unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].host, "a.example");
        assert_eq!(v[0].name, "Alpha");
        assert_eq!(v[0].port, 1234);
        assert_eq!(v[0].username, "bob");
    }

    #[test]
    fn parses_a_json_list_and_applies_defaults() {
        let json = r#"[
            {"host":"a.example"},
            {"host":"b.example","port":9999,"channel":"Riders"}
        ]"#;
        let v = parse_json(json, "rider").unwrap();
        assert_eq!(v.len(), 2);

        assert_eq!(v[0].port, DEFAULT_PORT, "port defaults");
        assert_eq!(v[0].username, "rider", "username falls back");
        assert_eq!(v[0].name, "a.example", "name defaults to the host");

        assert_eq!(v[1].port, 9999);
        assert_eq!(v[1].auto_join_channel.as_deref(), Some("Riders"));
    }

    fn with_proxy(kind: ProxyKind, tunnel_voice: bool) -> ServerProfile {
        let mut p = ServerProfile::new("Rig", "example.test", 64739, "rider");
        p.proxy_chain = vec![ProxySpec {
            kind,
            host: "10.0.0.1".into(),
            port: 1080,
            username: Some("rider".into()),
            password: Some("secret".into()),
            tunnel_voice,
        }];
        p
    }

    #[test]
    fn a_link_carries_the_proxy_a_server_was_given() {
        // A server that can only be reached through a proxy is not much of an
        // invitation without one.
        let url = build_url(&with_proxy(ProxyKind::Socks5, true), None, false);
        let back = parse_url(&url, "someone").unwrap();
        let proxy = back.proxy_chain.first().expect("the proxy came back");
        assert_eq!(proxy.kind, ProxyKind::Socks5);
        assert_eq!(proxy.host, "10.0.0.1");
        assert_eq!(proxy.port, 1080);
        assert!(proxy.tunnel_voice, "where voice goes is part of the route");
    }

    #[test]
    fn a_links_proxy_carries_credentials_only_when_the_password_does() {
        // The rider was asked one question about secrets; this is the same
        // answer, applied to the other one in the link.
        let profile = with_proxy(ProxyKind::HttpConnect, false);

        let without = build_url(&profile, None, false);
        assert!(!without.contains("secret"), "{without}");
        assert!(!without.contains("rider"), "{without}");
        assert!(without.contains("10.0.0.1"), "the address still travels");

        let with = build_url(&profile, None, true);
        let back = parse_url(&with, "someone").unwrap();
        let proxy = back.proxy_chain.first().unwrap();
        assert_eq!(proxy.username.as_deref(), Some("rider"));
        assert_eq!(proxy.password.as_deref(), Some("secret"));
    }

    #[test]
    fn a_server_without_a_proxy_shares_a_link_that_says_nothing_about_one() {
        let plain = ServerProfile::new("Rig", "example.test", 64739, "rider");
        let url = build_url(&plain, None, true);
        assert!(!url.contains("proxy"), "{url}");
        assert!(parse_url(&url, "s").unwrap().proxy_chain.is_empty());
    }

    #[test]
    fn a_profile_file_carries_a_proxy_the_same_way_a_link_does() {
        // One shape in both, so a rider can move one between them by copying.
        let json = build_json(&with_proxy(ProxyKind::Socks5, false), None, false).unwrap();
        assert!(json.contains("socks5://10.0.0.1:1080"), "{json}");
        let back = parse_json(&json, "someone").unwrap();
        assert_eq!(back[0].proxy_chain.len(), 1);
        assert_eq!(back[0].proxy_chain[0].port, 1080);
    }

    #[test]
    fn half_a_proxy_in_a_link_is_no_proxy() {
        // A route nobody can use is worse than none: without it the server is
        // still reachable directly, which is the safer of the two failures.
        for broken in [
            "socks5://10.0.0.1",   // no port
            "socks5://:1080",      // no host
            "ftp://10.0.0.1:1080", // not a proxy this app speaks
            "10.0.0.1:1080",       // no scheme
            "socks5://10.0.0.1:not-a-port",
        ] {
            let url = format!(
                "mumble://example.test:64739/?proxy={}",
                broken.replace("://", "%3A%2F%2F")
            );
            let back = parse_url(&url, "rider").unwrap();
            assert!(
                back.proxy_chain.is_empty(),
                "{broken} should not have become a proxy"
            );
        }
    }

    #[test]
    fn an_ipv6_proxy_survives_the_brackets() {
        let mut p = ServerProfile::new("Rig", "example.test", 64739, "rider");
        p.proxy_chain = vec![ProxySpec {
            kind: ProxyKind::Socks5,
            host: "2001:db8::1".into(),
            port: 1080,
            username: None,
            password: None,
            tunnel_voice: false,
        }];
        let back = parse_url(&build_url(&p, None, false), "rider").unwrap();
        assert_eq!(back.proxy_chain[0].host, "2001:db8::1");
        assert_eq!(back.proxy_chain[0].port, 1080);
    }

    #[test]
    fn a_proxy_link_round_trips_on_its_own() {
        let spec = ProxySpec {
            kind: ProxyKind::Socks5,
            host: "10.0.0.1".into(),
            port: 1080,
            username: Some("rider".into()),
            password: Some("secret".into()),
            tunnel_voice: true,
        };
        let link = build_proxy_url(&spec, true);
        assert!(link.starts_with("mumble-proxy://"), "{link}");
        let back = parse_proxy_url(&link).expect("it reads back");
        assert_eq!(back, spec);

        // And through the https wrapper a messenger will actually carry.
        let web = build_proxy_web_url(&spec, true);
        assert_eq!(parse_proxy_url(&web), Some(spec));
    }

    #[test]
    fn a_shared_proxy_keeps_its_credentials_to_itself_unless_asked() {
        let spec = ProxySpec {
            kind: ProxyKind::HttpConnect,
            host: "proxy.example".into(),
            port: 8000,
            username: Some("rider".into()),
            password: Some("secret".into()),
            tunnel_voice: false,
        };
        let link = build_proxy_url(&spec, false);
        assert!(!link.contains("secret"), "{link}");
        let back = parse_proxy_url(&link).unwrap();
        assert_eq!(back.host, "proxy.example");
        assert_eq!(back.username, None);
    }

    #[test]
    fn a_server_link_is_not_a_proxy_link_and_the_reverse() {
        // They look alike and mean entirely different things; confusing them
        // would add a server as a proxy, or a proxy as a server.
        assert!(parse_proxy_url("mumble://example.test:64738/").is_none());
        assert!(parse_proxy_url("https://example.com/#mumble://a.test/").is_none());
        assert!(parse_url("mumble-proxy://10.0.0.1:1080/?kind=socks5", "u").is_err());
    }

    #[test]
    fn json_without_a_usable_server_is_rejected() {
        assert!(parse_json("", "u").is_err());
        assert!(parse_json("not json", "u").is_err());
        assert!(parse_json("[]", "u").is_err());
        // Entries with a blank host are dropped, leaving nothing.
        assert!(parse_json(r#"[{"host":"  "}]"#, "u").is_err());
    }

    #[test]
    fn parse_any_accepts_both_forms() {
        assert_eq!(parse_any("mumble://h.example", "u").unwrap().len(), 1);
        assert_eq!(
            parse_any(r#"[{"host":"a"},{"host":"b"}]"#, "u")
                .unwrap()
                .len(),
            2
        );
        assert_eq!(parse_any(r#"{"host":"a"}"#, "u").unwrap().len(), 1);
    }

    #[test]
    fn built_links_round_trip_through_the_parser() {
        let mut p = ServerProfile::new("Sunday Ride", "voice.example.com", 64744, "alice");
        p.password = Some("s3cret pass".into());

        let url = build_url(&p, Some("Riders Lounge"), true);
        let back = parse_url(&url, "the-scanning-device").unwrap();

        assert_eq!(back.host, "voice.example.com");
        assert_eq!(back.port, 64744);
        // Not "alice". The link carries no name, so the scanning device's own
        // is what fills in — which is the whole point of leaving it out.
        assert_eq!(back.username, "the-scanning-device");
        assert_eq!(back.password.as_deref(), Some("s3cret pass"));
        assert_eq!(back.auto_join_channel.as_deref(), Some("Riders Lounge"));
        assert_eq!(back.name, "Sunday Ride");
    }

    #[test]
    fn shared_links_never_name_the_rider_who_shared_them() {
        let mut p = ServerProfile::new("Sunday Ride", "voice.example.com", 64744, "alice");
        p.password = Some("hunter2".into());

        for include_password in [true, false] {
            let url = build_url(&p, Some("Riders Lounge"), include_password);
            assert!(
                !url.contains("alice"),
                "the sharer's name leaked into a link: {url}"
            );
            let json = build_json(&p, Some("Riders Lounge"), include_password).unwrap();
            assert!(
                !json.contains("alice"),
                "the sharer's name leaked into a profile file: {json}"
            );
        }
    }

    #[test]
    fn a_password_survives_without_a_username_beside_it() {
        // `mumble://:secret@host` — empty userinfo user, which is ordinary URI
        // syntax but the one shape this format had never produced before the
        // username was dropped. If a parser mishandled it the password would
        // arrive as part of the host, and the link would be silently useless.
        let mut p = ServerProfile::new("S", "h.example", DEFAULT_PORT, "alice");
        p.password = Some("s3cret pass".into());

        let url = build_url(&p, None, true);
        assert!(url.starts_with("mumble://:"), "unexpected shape: {url}");

        let back = parse_url(&url, "fallback").unwrap();
        assert_eq!(back.host, "h.example");
        assert_eq!(back.port, DEFAULT_PORT);
        assert_eq!(back.password.as_deref(), Some("s3cret pass"));
        assert_eq!(back.username, "fallback");
    }

    /// The shareable form, which is what actually gets sent to somebody.
    #[test]
    fn a_web_invitation_round_trips() {
        let mut p = ServerProfile::new("Sunday Ride", "voice.example.com", 64744, "alice");
        p.password = Some("s3cret pass".into());

        let url = build_web_url(&p, Some("Riders"), true);
        assert!(url.starts_with(WEB_INVITE_BASE), "unexpected shape: {url}");
        // The payload rides in the fragment, which a browser never sends to the
        // server. If this ever moves into the path or the query, every
        // invitation starts appearing in somebody's access log.
        let (_, fragment) = url.split_once('#').expect("no fragment");
        assert!(fragment.starts_with("mumble://"), "fragment: {fragment}");

        let back = parse_url(&url, "fallback").unwrap();
        assert_eq!(back.host, "voice.example.com");
        assert_eq!(back.port, 64744);
        assert_eq!(back.password.as_deref(), Some("s3cret pass"));
        assert_eq!(back.auto_join_channel.as_deref(), Some("Riders"));
        assert_eq!(back.name, "Sunday Ride");
        // Still no username in an invitation; the device supplies its own.
        assert_eq!(back.username, "fallback");
    }

    /// A messenger or QR reader may hand the fragment back percent-encoded.
    #[test]
    fn a_web_invitation_survives_being_percent_encoded() {
        let p = ServerProfile::new("S", "h.example", DEFAULT_PORT, "alice");
        let encoded = format!("{WEB_INVITE_BASE}#mumble%3A%2F%2Fh.example%2FRiders");
        let back = parse_url(&encoded, "fallback").unwrap();
        assert_eq!(back.host, "h.example");
        assert_eq!(back.auto_join_channel.as_deref(), Some("Riders"));
        let _ = p;
    }

    /// Only our own invitation path unwraps a fragment. Any other https link is
    /// still an error rather than something to go looking inside.
    #[test]
    fn other_https_links_are_not_treated_as_invitations() {
        for url in [
            "https://example.com/join/#mumble://h.example/Riders",
            "https://zavitax.github.io/mumbleway/#mumble://h.example/Riders",
            "https://zavitax.github.io/mumbleway/join/#not-a-mumble-link",
            "https://zavitax.github.io/mumbleway/join/",
        ] {
            assert!(
                parse_url(url, "x").is_err(),
                "should not have been read as an invitation: {url}"
            );
        }
    }

    /// `parse_any` is what the paste box and the QR reader both go through.
    #[test]
    fn parse_any_takes_a_web_invitation() {
        let p = ServerProfile::new("S", "h.example", DEFAULT_PORT, "alice");
        let url = build_web_url(&p, Some("Riders"), false);
        let got = parse_any(&url, "rider").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].host, "h.example");
    }

    #[test]
    fn awkward_characters_survive_a_round_trip() {
        // Names with spaces, accents and separators are exactly what naive
        // string concatenation gets wrong.
        let mut p = ServerProfile::new("Café / Bar", "host.example", DEFAULT_PORT, "two words");
        p.password = Some("a:b@c/d".into());

        let url = build_url(&p, Some("Étage 2"), true);
        let back = parse_url(&url, "x").unwrap();

        assert_eq!(back.username, "x");
        assert_eq!(back.password.as_deref(), Some("a:b@c/d"));
        assert_eq!(back.auto_join_channel.as_deref(), Some("Étage 2"));
        assert_eq!(back.name, "Café / Bar");
        assert_eq!(back.host, "host.example");
    }

    #[test]
    fn passwords_are_omitted_unless_explicitly_included() {
        let mut p = ServerProfile::new("S", "h.example", DEFAULT_PORT, "u");
        p.password = Some("hunter2".into());

        let shared = build_url(&p, None, false);
        assert!(
            !shared.contains("hunter2"),
            "password leaked into a link that opted out: {shared}"
        );
        assert_eq!(parse_url(&shared, "x").unwrap().password, None);

        let json = build_json(&p, None, false).unwrap();
        assert!(
            !json.contains("hunter2"),
            "password leaked into a profile file"
        );
    }

    #[test]
    fn the_default_port_is_left_out_of_links() {
        let p = ServerProfile::new("S", "h.example", DEFAULT_PORT, "u");
        assert!(!build_url(&p, None, false).contains("64738"));

        let q = ServerProfile::new("S", "h.example", 1234, "u");
        assert!(build_url(&q, None, false).contains(":1234"));
    }

    #[test]
    fn built_profile_files_parse_back() {
        let mut p = ServerProfile::new("Team", "h.example", 4242, "bob");
        p.password = Some("pw".into());

        let json = build_json(&p, Some("Ops"), true).unwrap();
        let back = parse_json(&json, "x").unwrap();

        assert_eq!(back.len(), 1);
        assert_eq!(back[0].host, "h.example");
        assert_eq!(back[0].port, 4242);
        // The fallback, not "bob" — a profile file names a server, not a rider.
        assert_eq!(back[0].username, "x");
        assert_eq!(back[0].password.as_deref(), Some("pw"));
        assert_eq!(back[0].auto_join_channel.as_deref(), Some("Ops"));
    }

    #[test]
    fn shared_profiles_never_carry_our_pinned_certificate() {
        // Copying a trust decision onto someone else's device would launder it.
        let mut p = ServerProfile::new("S", "h.example", DEFAULT_PORT, "u");
        p.cert_fingerprint = Some("aa".repeat(32));

        let json = build_json(&p, None, true).unwrap();
        assert!(!json.contains(&"aa".repeat(32)));
        assert_eq!(parse_json(&json, "x").unwrap()[0].cert_fingerprint, None);
    }

    #[test]
    fn blank_username_in_a_link_falls_back() {
        // "mumble://@host" and "mumble://host" should behave the same.
        let p = parse_url("mumble://@host.example/", "rider").unwrap();
        assert_eq!(p.username, "rider");
    }
}
