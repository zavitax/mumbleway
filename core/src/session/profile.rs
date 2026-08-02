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
use crate::session::ServerProfile;

/// Mumble's default port, used when a link omits one.
pub const DEFAULT_PORT: u16 = 64738;

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

    let mut profile =
        ServerProfile::new(title.unwrap_or_else(|| host.clone()), host, port, username);
    profile.password = password;
    profile.auto_join_channel = channel;
    Ok(profile)
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

    if !profile.username.trim().is_empty() {
        url.push_str(&percent_encode(&profile.username));
        if include_password {
            if let Some(p) = profile.password.as_ref().filter(|p| !p.is_empty()) {
                url.push(':');
                url.push_str(&percent_encode(p));
            }
        }
        url.push('@');
    }

    url.push_str(&profile.host);
    if profile.port != DEFAULT_PORT {
        url.push_str(&format!(":{}", profile.port));
    }

    url.push('/');
    if let Some(c) = channel.filter(|c| !c.trim().is_empty()) {
        url.push_str(&percent_encode(c));
    }

    if !profile.name.trim().is_empty() && profile.name != profile.host {
        url.push_str(&format!("?title={}", percent_encode(&profile.name)));
    }
    url
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
        username: Some(profile.username.clone()),
        password: if include_password {
            profile.password.clone()
        } else {
            None
        },
        channel: channel.map(|c| c.to_string()),
        // Never shared: the pin is this device's own trust decision, and
        // copying it would launder it onto someone else's device.
        cert_fingerprint: None,
    };
    serde_json::to_string_pretty(&vec![entry])
        .map_err(|e| CoreError::Other(format!("could not build profile: {e}")))
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
        let back = parse_url(&url, "unused").unwrap();

        assert_eq!(back.host, "voice.example.com");
        assert_eq!(back.port, 64744);
        assert_eq!(back.username, "alice");
        assert_eq!(back.password.as_deref(), Some("s3cret pass"));
        assert_eq!(back.auto_join_channel.as_deref(), Some("Riders Lounge"));
        assert_eq!(back.name, "Sunday Ride");
    }

    #[test]
    fn awkward_characters_survive_a_round_trip() {
        // Names with spaces, accents and separators are exactly what naive
        // string concatenation gets wrong.
        let mut p = ServerProfile::new("Café / Bar", "host.example", DEFAULT_PORT, "two words");
        p.password = Some("a:b@c/d".into());

        let url = build_url(&p, Some("Étage 2"), true);
        let back = parse_url(&url, "x").unwrap();

        assert_eq!(back.username, "two words");
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
        assert_eq!(back[0].username, "bob");
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
