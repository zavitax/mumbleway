//! What a real Mumble server actually says, against a real one.
//!
//! Everything this client learned about servers in the last round — the
//! bandwidth allowance, the permission masks, the per-rider statistics, the
//! client handshake — was read out of Murmur's source and checked by unit tests
//! against values typed in by hand. That proves the arithmetic and proves
//! nothing about the wire.
//!
//! These are `#[ignore]`d because they need a server. Start one and point them
//! at it:
//!
//! ```text
//! docker run -d --name mw-murmur -p 64739:64738/tcp -p 64739:64738/udp \
//!     -e MUMBLE_CONFIG_BANDWIDTH=32000 mumblevoip/mumble-server:latest
//! MW_LIVE=127.0.0.1:64739 cargo test --test live_server -- --ignored --nocapture
//! ```
//!
//! `MW_LIVE_BANDWIDTH` tells the test what the server was configured with, so
//! it can check the figure that arrives is that one rather than merely *a*
//! number.

use std::time::Duration;

use mumbleway_core::audio::bandwidth;
use mumbleway_core::net::tls::Identity;
use mumbleway_core::session::manager::{SessionManager, TaggedEvent};
use mumbleway_core::session::{AudioBridge, ConnectionState, ServerProfile, SessionEvent};
use tokio::sync::mpsc;

/// Where the server is, or `None` to skip.
fn live_address() -> Option<(String, u16)> {
    let raw = std::env::var("MW_LIVE").ok()?;
    let (host, port) = raw.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

/// What the server was configured to allow, when the caller said.
fn expected_bandwidth() -> Option<u32> {
    std::env::var("MW_LIVE_BANDWIDTH").ok()?.parse().ok()
}

/// An audio bridge that goes nowhere.
///
/// These tests are about the control channel. Nothing encodes, so the outgoing
/// side is never fed; incoming voice is accepted and dropped.
fn silent_bridge() -> AudioBridge {
    let (_out_tx, out_rx) = mpsc::channel(8);
    let (in_tx, _in_rx) = mpsc::channel(64);
    // Leaked deliberately: dropping the far ends would close the channels and
    // the session would see its audio path collapse, which is not what is
    // under test here.
    Box::leak(Box::new(_out_tx));
    Box::leak(Box::new(_in_rx));
    AudioBridge {
        outgoing: out_rx,
        incoming: in_tx,
    }
}

/// Connects `names` to the live server and collects events for `secs`.
async fn gather(names: &[&str], secs: u64) -> Vec<TaggedEvent> {
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live test").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    for name in names {
        let mut profile = ServerProfile::new("live", host.clone(), port, *name);
        profile.id = (*name).to_string();
        let id = manager
            .add(profile, silent_bridge())
            .expect("session added");
        // A session sits idle until told. Adding one only builds it — which is
        // how the app works too, and is what an earlier run of this test got
        // wrong: no command, no dial, and a server log with nothing in it.
        manager
            .send(&id, mumbleway_core::session::SessionCommand::Connect)
            .await
            .expect("connect command");
    }

    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Some(event)) => seen.push(event),
            Ok(None) => break,
            Err(_) => break,
        }
    }
    manager.shutdown_all().await;
    seen
}

/// Skips with a word rather than passing silently when no server is configured.
macro_rules! require_server {
    () => {
        if live_address().is_none() {
            eprintln!("MW_LIVE is not set; skipping (see this file's header)");
            return;
        }
    };
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_real_server_hands_over_its_bandwidth_allowance() {
    require_server!();
    let events = gather(&["rider-one"], 12).await;

    let caps: Vec<u32> = events
        .iter()
        .filter_map(|e| match e.event {
            SessionEvent::BandwidthCap(bps) => Some(bps),
            _ => None,
        })
        .collect();

    println!("bandwidth allowances reported: {caps:?}");
    assert!(
        !caps.is_empty(),
        "no allowance arrived at all — ServerSync and ServerConfig both ignored?"
    );

    if let Some(want) = expected_bandwidth() {
        assert!(
            caps.contains(&want),
            "server was configured for {want} bit/s, client was told {caps:?}"
        );

        // And the arithmetic the encoder is driven by, against the real figure.
        let budget = bandwidth::fit(Some(want), bandwidth::preferred_bps());
        println!(
            "budget from a real {want} bit/s allowance: {} bit/s, capped={}, below_floor={}",
            budget.bitrate_bps, budget.capped, budget.below_floor
        );
        assert!(
            budget.bitrate_bps + bandwidth::overhead_bps() <= want,
            "what we would send does not fit what the server allows"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_real_server_answers_what_this_rider_may_do() {
    require_server!();
    let events = gather(&["rider-one"], 12).await;

    let rights: Vec<_> = events
        .iter()
        .filter_map(|e| match e.event {
            SessionEvent::Rights(r) => Some(r),
            _ => None,
        })
        .collect();

    println!("rights reported: {rights:?}");
    let last = rights.last().expect("no PermissionQuery answer arrived");
    assert!(last.known, "an answer arrived but reads as 'not asked yet'");
    // What a default Murmur grants an ordinary unregistered user.
    assert!(last.speak, "a default server lets anybody speak");
    assert!(
        !last.ban,
        "an ordinary user was told they may ban — the root mask is being read wrong"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn two_riders_see_each_other_and_the_servers_measurements() {
    require_server!();
    // Two sessions on one server: the second is what makes UserStats and the
    // MumbleWay handshake testable at all, since both are about somebody else.
    let events = gather(&["rider-one", "rider-two"], 20).await;

    let mut with_quality = 0;
    let mut mumbleway_badges = 0;
    for event in &events {
        if let SessionEvent::Users(users) = &event.event {
            for u in users {
                if let Some(q) = u.quality {
                    with_quality += 1;
                    println!(
                        "{} sees {}: ping {:.1} ms (udp={}), loss up {:.3} down {:.3}, window {} s",
                        event.server_id,
                        u.name,
                        q.ping_ms,
                        q.udp,
                        q.loss_up,
                        q.loss_down,
                        q.window_secs
                    );
                }
                if u.mumbleway.is_some() {
                    mumbleway_badges += 1;
                }
            }
        }
    }

    println!("roster entries carrying a measurement: {with_quality}");
    println!("roster entries badged as MumbleWay: {mumbleway_badges}");
    assert!(
        with_quality > 0,
        "no UserStats reply was turned into a quality figure"
    );
    assert!(
        mumbleway_badges > 0,
        "the plugin-data handshake produced no badge on a 1.4+ server"
    );

    // **This is the assertion that caught a real bug.** Mumble servers do not
    // measure a client's ping; they copy the figures out of its Ping messages
    // and hand them on. This client sent none, so every rider read as 0 ms to
    // everybody — in this app's roster and in the official client alike — and
    // every unit test passed, because the server was faithfully returning the
    // nothing it had been given.
    let best_ping = events
        .iter()
        .filter_map(|e| match &e.event {
            SessionEvent::Users(users) => Some(users),
            _ => None,
        })
        .flatten()
        .filter_map(|u| u.quality)
        .map(|q| q.ping_ms)
        .fold(0.0f32, f32::max);
    println!("highest ping any rider reported: {best_ping} ms");
    assert!(
        best_ping > 0.0,
        "every rider's ping reads zero — the Ping message is not reporting ours"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn one_rider_can_ask_another_to_mute() {
    require_server!();
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live test").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    for name in ["asker", "target"] {
        let mut profile = ServerProfile::new("live", host.clone(), port, name);
        profile.id = name.to_string();
        let id = manager.add(profile, silent_bridge()).expect("added");
        manager
            .send(&id, mumbleway_core::session::SessionCommand::Connect)
            .await
            .expect("connect");
    }

    // Long enough for both to connect and to have exchanged hellos, since the
    // request is only sent to a peer whose hello listed the capability.
    let mut target_session = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(14);
    let mut requests = Vec::new();
    let mut asked = false;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };

        if let SessionEvent::Users(users) = &event.event {
            if event.server_id == "asker" {
                if let Some(t) = users
                    .iter()
                    .find(|u| u.name == "target" && u.mumbleway.is_some())
                {
                    target_session = Some(t.session);
                }
            }
        }
        if let SessionEvent::RemoteMuteRequested { mute, by } = &event.event {
            println!("{} was asked to mute={mute} by {by}", event.server_id);
            requests.push((event.server_id.clone(), *mute));
        }

        // Once the target has identified itself as MumbleWay, ask it to mute.
        if let (false, Some(session)) = (asked, target_session) {
            asked = true;
            manager
                .send(
                    "asker",
                    mumbleway_core::session::SessionCommand::SetUserServerMute {
                        session,
                        muted: true,
                    },
                )
                .await
                .expect("mute command");
        }
    }
    manager.shutdown_all().await;

    assert!(
        asked,
        "the target never identified itself as a MumbleWay peer"
    );
    assert!(
        requests.iter().any(|(who, mute)| who == "target" && *mute),
        "the mute request never arrived: {requests:?}"
    );
}

/// The admin paths: reading the ban list, and what the server does *not* say.
///
/// **Murmur never answers a permission query for the SuperUser.**
/// `Server::sendClientPermission` opens with `if (u->iId == 0) return;`, so an
/// admin logged in that way is told nothing about their own permissions. This
/// client treats "no answer" as "offer everything" — see
/// `permissions::Rights::known` — which is right by construction here, since
/// SuperUser may do everything. It is asserted rather than assumed, because the
/// opposite reading would have greyed out every moderation action for the one
/// account that can use them.
/// Coming back to the channel the rider was in, not to the root.
///
/// Needs a channel to move to: `MW_LIVE_CHANNEL` names one that exists on the
/// server. Create one with
///
/// ```text
/// insert into channels (server_id, channel_id, parent_id, name, inheritacl)
/// values (1, 1, 0, 'Garage', 1);
/// ```
///
/// What is checked is the whole round trip the rider experiences: join a
/// channel, lose the session, come back, and be where they were — which is the
/// core's own auto-join doing it, without the app having to notice the drop.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_reconnect_returns_to_the_channel_the_rider_was_in() {
    require_server!();
    let Ok(channel) = std::env::var("MW_LIVE_CHANNEL") else {
        eprintln!("MW_LIVE_CHANNEL is not set; skipping");
        return;
    };
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live test").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    let mut profile = ServerProfile::new("live", host, port, "wanderer");
    profile.id = "wanderer".into();
    let id = manager.add(profile, silent_bridge()).expect("added");
    manager
        .send(&id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    let mut me = None;
    let mut channels: Vec<(u32, String)> = Vec::new();
    let mut moved = false;
    let mut dropped = false;
    let mut where_after_reconnect = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);

    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        match &event.event {
            SessionEvent::SelfSession(s) => me = Some(*s),
            SessionEvent::Channels(list) => {
                channels = list.iter().map(|c| (c.id, c.name.clone())).collect();
            }
            SessionEvent::Users(users) => {
                let Some(me) = me else { continue };
                let Some(here) = users.iter().find(|u| u.session == me) else {
                    continue;
                };
                let name = channels
                    .iter()
                    .find(|(id, _)| *id == here.channel_id)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();

                if !moved {
                    // Move, the way the app does, and tell the session to
                    // treat it as the channel to come back to.
                    if let Some((target, _)) = channels.iter().find(|(_, n)| *n == channel) {
                        moved = true;
                        manager
                            .send(
                                &id,
                                mumbleway_core::session::SessionCommand::JoinChannel(*target),
                            )
                            .await
                            .expect("join");
                        manager
                            .send(
                                &id,
                                mumbleway_core::session::SessionCommand::SetDefaultChannel(Some(
                                    channel.clone(),
                                )),
                            )
                            .await
                            .expect("remember");
                    }
                } else if !dropped && name == channel {
                    // There now. Pull the session down and let it come back:
                    // Disconnect then Connect is what a dropped link looks
                    // like from the session's point of view.
                    dropped = true;
                    println!("in {name}; dropping the session");
                    manager
                        .send(&id, mumbleway_core::session::SessionCommand::Disconnect)
                        .await
                        .expect("disconnect");
                    manager
                        .send(&id, mumbleway_core::session::SessionCommand::Connect)
                        .await
                        .expect("reconnect");
                } else if dropped && !name.is_empty() {
                    where_after_reconnect = Some(name);
                }
            }
            _ => {}
        }
    }
    manager.shutdown_all().await;

    println!("after reconnecting, the rider is in: {where_after_reconnect:?}");
    assert!(moved, "the named channel was not on the server");
    assert!(dropped, "never reached the channel to drop from");
    assert_eq!(
        where_after_reconnect.as_deref(),
        Some(channel.as_str()),
        "a reconnect left the rider somewhere other than where they were"
    );
}

/// Being silenced by the channel itself, which is the one way of going
/// inaudible that nothing else on a rider's screen knows about.
///
/// Set `MW_LIVE_EXPECT_SUPPRESSED=1` when the server has been configured to
/// deny Speak — otherwise this asserts the opposite, which is worth checking
/// too: an ordinary server must not have riders reading as suppressed.
///
/// To make a server do it, deny Speak to `all` on the root channel and restart:
///
/// ```text
/// insert into acl (server_id, channel_id, priority, user_id, group_name,
///                  apply_here, apply_sub, grantpriv, revokepriv)
/// values (1, 0, 4, null, 'all', 1, 1, 0, 8);
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_channel_that_will_not_carry_a_voice_says_so() {
    require_server!();
    let expect_suppressed = std::env::var("MW_LIVE_EXPECT_SUPPRESSED").is_ok();
    let events = gather(&["rider-one"], 12).await;

    let announcements: Vec<bool> = events
        .iter()
        .filter_map(|e| match e.event {
            SessionEvent::SelfSuppressed(v) => Some(v),
            _ => None,
        })
        .collect();

    // What the roster says about us, which is the other half: a rider the
    // server has silenced must not read as merely quiet.
    let mut own_label = None;
    for event in &events {
        if let SessionEvent::Users(users) = &event.event {
            if let Some(me) = users.iter().find(|u| u.name == "rider-one") {
                own_label = Some((me.suppress, me.status_label(), me.is_audible()));
            }
        }
    }

    println!("suppression announcements: {announcements:?}");
    println!("our own roster entry: {own_label:?}");

    let (suppressed, label, audible) = own_label.expect("we never appeared in our own roster");
    if expect_suppressed {
        assert!(
            announcements.contains(&true),
            "the server is denying Speak and nothing was announced"
        );
        assert!(suppressed, "the roster does not carry it");
        assert_eq!(label, "suppressed", "and it must not read as merely silent");
        assert!(
            !audible,
            "a rider whose every packet is discarded is not audible"
        );
    } else {
        assert!(
            announcements.iter().all(|v| !v),
            "an ordinary server suppressed nobody, yet something was announced"
        );
        assert!(!suppressed);
        assert!(audible);
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live server AND MW_LIVE_SUPERUSER set to its password"]
async fn an_admin_can_read_the_ban_list_and_is_told_nothing_about_permissions() {
    require_server!();
    let Ok(password) = std::env::var("MW_LIVE_SUPERUSER") else {
        eprintln!("MW_LIVE_SUPERUSER is not set; skipping the admin checks");
        return;
    };
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live admin").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    let mut profile = ServerProfile::new("live", host, port, "SuperUser");
    profile.id = "admin".into();
    profile.password = Some(password);
    let id = manager.add(profile, silent_bridge()).expect("added");
    manager
        .send(&id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    let mut rights = None;
    let mut ban_lists: Vec<usize> = Vec::new();
    let mut asked = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(14);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        match event.event {
            SessionEvent::Rights(r) => rights = Some(r),
            SessionEvent::Bans(list) => {
                println!("ban list: {} entries", list.len());
                ban_lists.push(list.len());
            }
            // Asked once there is a session to ask on, rather than after a
            // permission answer that this account never receives.
            SessionEvent::State(ConnectionState::Connected) if !asked => {
                asked = true;
                manager
                    .send(&id, mumbleway_core::session::SessionCommand::RequestBans)
                    .await
                    .expect("ban list request");
            }
            _ => {}
        }
    }
    manager.shutdown_all().await;

    println!("rights reported for SuperUser: {rights:?}");
    assert!(asked, "never reached a connected session");
    assert!(
        !ban_lists.is_empty(),
        "the ban list request went unanswered — BanList is not being handled"
    );
    assert!(
        rights.is_none_or(|r| !r.known),
        "Murmur is documented not to answer a SuperUser's permission query; \
         if it now does, the interface can stop guessing for this account"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_rider_reaches_the_connected_state_at_all() {
    require_server!();
    let events = gather(&["rider-one"], 10).await;
    let states: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.event {
            SessionEvent::State(s) => Some(format!("{s:?}")),
            _ => None,
        })
        .collect();
    println!("connection states: {states:?}");
    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, SessionEvent::State(ConnectionState::Connected))),
        "never connected: {states:?}"
    );
}
