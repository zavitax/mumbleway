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
/// A channel that only a token opens, opened with one.
///
/// **A Mumble channel has no password.** What it has is an ACL that grants
/// entry to a group, and a token is a string that puts the rider in a group of
/// that name — so "the password for the clubhouse" is a token spelled
/// `clubhouse`. `MW_LIVE_TOKEN_CHANNEL` names such a channel and
/// `MW_LIVE_TOKEN` the token that opens it; set both up with
///
/// ```text
/// insert into acl (...) values (1, 2, 1, null, 'all', 1, 1, 0, 4);   -- deny Enter
/// insert into acl (...) values (1, 2, 2, null, 'vip', 1, 1, 4, 0);   -- grant @vip
/// ```
///
/// What is checked is both halves: refused without the token, and in with it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_token_opens_a_channel_that_is_otherwise_shut() {
    require_server!();
    let (Ok(channel), Ok(token)) = (
        std::env::var("MW_LIVE_TOKEN_CHANNEL"),
        std::env::var("MW_LIVE_TOKEN"),
    ) else {
        eprintln!("MW_LIVE_TOKEN_CHANNEL / MW_LIVE_TOKEN are not set; skipping");
        return;
    };

    /// Connects with these tokens and reports whether the channel let us in.
    async fn try_entering(
        host: &str,
        port: u16,
        channel: &str,
        tokens: Vec<String>,
    ) -> (bool, Vec<String>) {
        let (tx, mut rx) = mpsc::channel(4096);
        let identity = Identity::generate("MumbleWay live token").expect("identity");
        let mut manager =
            SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");
        let mut profile = ServerProfile::new("live", host, port, "token-holder");
        profile.id = "token".into();
        profile.access_tokens = tokens;
        let id = manager.add(profile, silent_bridge()).expect("added");
        manager
            .send(&id, mumbleway_core::session::SessionCommand::Connect)
            .await
            .expect("connect");

        let mut me = None;
        let mut channels: Vec<(u32, String)> = Vec::new();
        let mut asked = false;
        let mut inside = false;
        let mut refusals = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
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
                SessionEvent::Refused { reason, kind } => {
                    refusals.push(format!("kind {kind}: {reason}"));
                }
                SessionEvent::Users(users) => {
                    let Some(me) = me else { continue };
                    let target = channels
                        .iter()
                        .find(|(_, n)| n == channel)
                        .map(|(id, _)| *id);
                    if let (false, Some(to)) = (asked, target) {
                        asked = true;
                        manager
                            .send(
                                &id,
                                mumbleway_core::session::SessionCommand::JoinChannel(to),
                            )
                            .await
                            .expect("join");
                    }
                    if let (Some(to), Some(u)) = (target, users.iter().find(|u| u.session == me)) {
                        if u.channel_id == to {
                            inside = true;
                        }
                    }
                }
                _ => {}
            }
        }
        manager.shutdown_all().await;
        (inside, refusals)
    }

    let (host, port) = live_address().expect("MW_LIVE");

    let (without, refusals) = try_entering(&host, port, &channel, Vec::new()).await;
    println!("without a token: inside={without}, refusals={refusals:?}");
    assert!(
        !without,
        "the channel let us in without the token, so this proves nothing"
    );
    assert!(
        !refusals.is_empty(),
        "the server refused silently, which would hide the reason from a rider"
    );

    let (with, refusals) = try_entering(&host, port, &channel, vec![token]).await;
    println!("with the token: inside={with}, refusals={refusals:?}");
    assert!(with, "the token did not open the channel");
}

/// Hearing a channel without joining it.
///
/// A rider stays where they are and their voice still goes to their own
/// channel; what changes is what reaches their ears. `MW_LIVE_CHANNEL` names
/// the channel to listen to.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_rider_can_listen_to_a_channel_without_joining_it() {
    require_server!();
    let Ok(channel) = std::env::var("MW_LIVE_CHANNEL") else {
        eprintln!("MW_LIVE_CHANNEL is not set; skipping");
        return;
    };
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live listen").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");
    let mut profile = ServerProfile::new("live", host, port, "listener");
    profile.id = "listener".into();
    let id = manager.add(profile, silent_bridge()).expect("added");
    manager
        .send(&id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    use mumbleway_core::session::SessionCommand as C;

    let mut me = None;
    // Filled from the first channel listing; never read before then, which is
    // why it starts empty rather than as a value nobody uses.
    let mut channels: Vec<(u32, String)>;
    let mut target = None;
    let mut asked = false;
    let mut stopped = false;
    let mut listening_states: Vec<Vec<u32>> = Vec::new();
    let mut own_channel = None;
    let mut refusals = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(18);

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
                target = channels
                    .iter()
                    .find(|(_, n)| *n == channel)
                    .map(|(id, _)| *id);
            }
            SessionEvent::Refused { reason, kind } => {
                refusals.push(format!("kind {kind}: {reason}"));
            }
            SessionEvent::Users(users) => {
                if let (Some(me), Some(u)) = (me, users.iter().find(|u| Some(u.session) == me)) {
                    own_channel = Some(u.channel_id);
                    let _ = me;
                }
                if let (false, Some(to)) = (asked, target) {
                    asked = true;
                    manager
                        .send(
                            &id,
                            C::SetListening {
                                add: vec![to],
                                remove: Vec::new(),
                            },
                        )
                        .await
                        .expect("listen");
                }
            }
            SessionEvent::Listening(now) => {
                println!("listening to: {now:?}");
                listening_states.push(now.clone());
                // Having started, stop again — the half that is easy to get
                // wrong, since the server reports changes rather than a state.
                if !stopped && !now.is_empty() {
                    stopped = true;
                    manager
                        .send(
                            &id,
                            C::SetListening {
                                add: Vec::new(),
                                remove: now.clone(),
                            },
                        )
                        .await
                        .expect("stop listening");
                }
            }
            _ => {}
        }
    }
    manager.shutdown_all().await;

    println!("refusals: {refusals:?}");
    println!("own channel throughout: {own_channel:?}");
    let listened = target.expect("the channel to listen to was not on the server");
    assert!(
        listening_states.iter().any(|s| s.contains(&listened)),
        "the server never confirmed the listen: {listening_states:?}"
    );
    assert!(
        listening_states.last().is_some_and(|s| s.is_empty()),
        "stopping did not take: {listening_states:?}"
    );
    assert_ne!(
        own_channel,
        Some(listened),
        "listening moved the rider, which is the one thing it must not do"
    );
}

/// Asking about another rider answers, even when the server withholds the half
/// that only an admin may see.
///
/// A server tells only an administrator the client version, the address and the
/// certificate. This client used to report the answer *only* when one of those
/// was in it, so an ordinary rider asking about somebody got a reply the client
/// threw away and a dialog that said "Asking the server…" for ever. What is
/// pinned here is that the answer arrives at all; what is in it is the server's
/// business.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn asking_about_a_rider_answers_even_when_the_server_withholds_it() {
    require_server!();
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live details").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    let mut ids = Vec::new();
    for name in ["asker", "asked-about"] {
        let mut profile = ServerProfile::new("live", host.clone(), port, name);
        profile.id = name.to_string();
        let id = manager.add(profile, silent_bridge()).expect("added");
        manager
            .send(&id, mumbleway_core::session::SessionCommand::Connect)
            .await
            .expect("connect");
        ids.push(id);
    }

    use mumbleway_core::session::SessionCommand as C;

    let mut target = None;
    let mut asked = false;
    let mut answer = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);

    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        // Only what the asker sees; the other session is here to be asked about.
        if event.server_id != "asker" {
            continue;
        }
        match &event.event {
            SessionEvent::Users(users) => {
                target = users
                    .iter()
                    .find(|u| u.name == "asked-about")
                    .map(|u| u.session);
            }
            SessionEvent::UserDetails(d) => {
                println!(
                    "details for {}: client {:?}, address {:?}, online {}s",
                    d.session, d.release, d.address, d.online_secs
                );
                answer = Some(d.clone());
                break;
            }
            _ => {}
        }

        if let (false, Some(who)) = (asked, target) {
            asked = true;
            manager
                .send(&ids[0], C::RequestUserDetails(who))
                .await
                .expect("ask");
        }
    }
    manager.shutdown_all().await;

    let who = target.expect("the other rider never appeared in the roster");
    let answer = answer.expect("no answer at all — the dialog would still be waiting");
    assert_eq!(answer.session, who, "the answer was about somebody else");
}

/// A rider closing their own microphone reaches the other riders as
/// `self_mute`, not as a mute somebody imposed.
///
/// The roster draws those differently — one is somebody choosing not to talk,
/// the other is somebody who has been stopped — so the two flags have to arrive
/// apart. Pinned live because every link in it is the server's: this client
/// sends `UserState.self_mute`, the server relays it to everybody else, and the
/// other client reads it off their roster entry.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_rider_muting_themselves_is_told_apart_from_one_an_admin_muted() {
    require_server!();
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live self-mute").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    let mut ids = Vec::new();
    for name in ["quiet-one", "watcher"] {
        let mut profile = ServerProfile::new("live", host.clone(), port, name);
        profile.id = name.to_string();
        let id = manager.add(profile, silent_bridge()).expect("added");
        manager
            .send(&id, mumbleway_core::session::SessionCommand::Connect)
            .await
            .expect("connect");
        ids.push(id);
    }

    use mumbleway_core::session::SessionCommand as C;

    let mut asked = false;
    let mut seen_them = false;
    let mut muted_themselves = false;
    let mut muted_by_somebody = false;
    let mut deafened_themselves = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);

    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        // Only what the watcher sees: the other session is the one muting
        // itself, and its own roster entry would prove nothing about the wire.
        if event.server_id != "watcher" {
            continue;
        }
        if let SessionEvent::Users(users) = &event.event {
            if let Some(them) = users.iter().find(|u| u.name == "quiet-one") {
                seen_them = true;
                if them.self_mute {
                    muted_themselves = true;
                    muted_by_somebody = them.mute;
                }
                if them.self_deaf {
                    deafened_themselves = true;
                }
                if muted_themselves && deafened_themselves {
                    break;
                }
            }
        }
        if !asked && seen_them {
            asked = true;
            manager
                .send(&ids[0], C::SetSelfMute(true))
                .await
                .expect("mute themselves");
            // And their hearing, which travels the same way and used to travel
            // nowhere at all: deafening is local, so without telling anybody a
            // channel keeps talking to somebody who cannot hear it.
            manager
                .send(&ids[0], C::SetSelfDeaf(true))
                .await
                .expect("deafen themselves");
        }
    }
    manager.shutdown_all().await;

    assert!(seen_them, "the other rider never appeared in the roster");
    assert!(
        muted_themselves,
        "closing their own microphone never reached the other rider"
    );
    assert!(
        !muted_by_somebody,
        "it arrived as a mute somebody imposed, which is a different thing"
    );
    assert!(
        deafened_themselves,
        "turning their own sound off never reached the other rider"
    );
}

/// The count beside a channel follows the rider into it.
///
/// Occupancy is not sent by the server: it is counted from the roster, so it is
/// only as fresh as the last channel list this client emitted. It used to be
/// emitted when a *channel* changed and never when a rider moved — so on a
/// server where nobody creates channels, the numbers beside the channels kept
/// whatever they said on connect, which is what this pins.
///
/// What is asserted is the invariant rather than a particular number: after the
/// rider has moved, a channel list arrives and its count for that channel
/// agrees with the roster. Anybody else on the test server may be moving about
/// at the same time, and a figure pinned by hand would be theirs as much as
/// ours. `MW_LIVE_CHANNEL` names a channel to move into.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn the_count_beside_a_channel_follows_the_rider() {
    require_server!();
    let Ok(channel) = std::env::var("MW_LIVE_CHANNEL") else {
        eprintln!("MW_LIVE_CHANNEL is not set; skipping");
        return;
    };
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live occupancy").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");
    let mut profile = ServerProfile::new("live", host, port, "counter");
    profile.id = "counter".into();
    let id = manager.add(profile, silent_bridge()).expect("added");
    manager
        .send(&id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    use mumbleway_core::session::SessionCommand as C;

    let mut me = None;
    let mut target = None;
    let mut moved = false;
    let mut here = None;
    let mut roster_here = 0;
    let mut agreed: Option<(u32, u32)> = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);

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
            SessionEvent::Users(users) => {
                if let Some(mine) = me {
                    here = users
                        .iter()
                        .find(|u| u.session == mine)
                        .map(|u| u.channel_id);
                }
                if let Some(to) = target {
                    roster_here = users.iter().filter(|u| u.channel_id == to).count() as u32;
                }
            }
            SessionEvent::Channels(list) => {
                if let Some(c) = list.iter().find(|c| c.name == channel) {
                    target = Some(c.id);
                    println!(
                        "{} holds {} | roster says {} | we are in {:?}",
                        c.name, c.user_count, roster_here, here
                    );
                    // Only once the rider is in it: before the move the list is
                    // allowed to be about a room they are not standing in.
                    if moved && here == Some(c.id) {
                        agreed = Some((c.user_count, roster_here));
                        break;
                    }
                }
            }
            _ => {}
        }

        if let (false, Some(to), true) = (moved, target, here.is_some()) {
            moved = true;
            manager.send(&id, C::JoinChannel(to)).await.expect("join");
        }
    }
    manager.shutdown_all().await;

    assert!(target.is_some(), "{channel} is not on this server");
    let (counted, roster) = agreed.expect(
        "no channel list arrived after the rider moved — the counts beside the \
         channels are stale, which is the bug this test exists for",
    );
    assert_eq!(
        counted, roster,
        "the channel list says {counted} and the roster says {roster}"
    );
    assert!(
        roster >= 1,
        "the roster does not have us in the channel we joined"
    );
}

/// What a rider does in an ordinary session, against a real server.
///
/// Notes, text messages, muting themselves, deafening themselves, and moving
/// about — each of these is a message this client sends and a flag it reads,
/// and every one of them was only ever checked against a unit test before.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn two_riders_do_the_ordinary_things() {
    require_server!();
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live test").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    for name in ["talker", "listener"] {
        let mut profile = ServerProfile::new("live", host.clone(), port, name);
        profile.id = name.to_string();
        let id = manager.add(profile, silent_bridge()).expect("added");
        manager
            .send(&id, mumbleway_core::session::SessionCommand::Connect)
            .await
            .expect("connect");
    }

    use mumbleway_core::session::SessionCommand as C;
    const NOTE: &str = "On the A9 heading north";
    const SAID: &str = "radio check";

    let mut acted = false;
    let mut note_seen = false;
    let mut text_seen = false;
    let mut self_mute_seen = false;
    let mut self_deaf_seen = false;
    let mut suggestions: Vec<(Option<bool>, Option<bool>)> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);

    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        match &event.event {
            SessionEvent::SelfSession(_) if event.server_id == "talker" && !acted => {
                acted = true;
                manager
                    .send("talker", C::SetComment(NOTE.into()))
                    .await
                    .ok();
                manager.send("talker", C::SetSelfMute(true)).await.ok();
                manager.send("talker", C::SetSelfDeaf(true)).await.ok();
                manager
                    .send(
                        "talker",
                        C::SendText {
                            channel_id: None,
                            message: SAID.into(),
                        },
                    )
                    .await
                    .ok();
            }
            SessionEvent::ServerSuggests {
                push_to_talk,
                positional,
            } => {
                println!(
                    "server suggests: push_to_talk={push_to_talk:?} positional={positional:?}"
                );
                suggestions.push((*push_to_talk, *positional));
            }
            // What the *other* rider sees is the half that matters: a note and
            // a mute are only worth anything if they reach somebody else.
            SessionEvent::Users(users) if event.server_id == "listener" => {
                if let Some(them) = users.iter().find(|u| u.name == "talker") {
                    println!(
                        "listener sees talker: comment={:?} self_mute={} self_deaf={}",
                        them.comment, them.self_mute, them.self_deaf
                    );
                    if them.comment == NOTE {
                        note_seen = true;
                    }
                    if them.self_mute {
                        self_mute_seen = true;
                    }
                    if them.self_deaf {
                        self_deaf_seen = true;
                    }
                }
            }
            SessionEvent::Text { from, message } if event.server_id == "listener" => {
                println!("listener heard {from}: {message}");
                if message == SAID {
                    text_seen = true;
                }
            }
            _ => {}
        }
    }
    manager.shutdown_all().await;

    assert!(note_seen, "a rider's note never reached the other rider");
    assert!(text_seen, "a text message never arrived");
    assert!(
        self_mute_seen,
        "muting themselves was invisible to everybody else"
    );
    assert!(self_deaf_seen, "deafening themselves was invisible too");
    if std::env::var("MW_LIVE_EXPECT_SUGGESTIONS").is_ok() {
        assert!(
            suggestions
                .iter()
                .any(|(ptt, pos)| *ptt == Some(true) || *pos == Some(true)),
            "the server was configured to suggest something and said nothing"
        );
    }
}

/// Removing somebody: moved, kicked, banned, and the ban lifted again.
///
/// The destructive half of moderation, which has had no live run at all. Each
/// of these is irreversible from the target's point of view, and a ban is the
/// one that outlasts the session — so the test puts the server back as it
/// found it and checks that it did.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live server AND MW_LIVE_SUPERUSER set to its password"]
async fn an_admin_moves_kicks_and_bans_somebody() {
    require_server!();
    let Ok(password) = std::env::var("MW_LIVE_SUPERUSER") else {
        eprintln!("MW_LIVE_SUPERUSER is not set; skipping");
        return;
    };
    let Ok(channel) = std::env::var("MW_LIVE_CHANNEL") else {
        eprintln!("MW_LIVE_CHANNEL is not set; skipping");
        return;
    };
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live admin").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    let mut admin = ServerProfile::new("live", host.clone(), port, "SuperUser");
    admin.id = "admin".into();
    admin.password = Some(password);
    let admin_id = manager.add(admin, silent_bridge()).expect("added");
    manager
        .send(&admin_id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    let mut victim = ServerProfile::new("live", host, port, "tyre-kicker");
    victim.id = "victim".into();
    let victim_id = manager.add(victim, silent_bridge()).expect("added");
    manager
        .send(&victim_id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    use mumbleway_core::session::SessionCommand as C;

    let mut channels: Vec<(u32, String)> = Vec::new();
    let mut target: Option<u32> = None;
    let mut moved_them = false;
    let mut saw_them_moved = false;
    let mut banned = false;
    let mut ban_names: Vec<String> = Vec::new();
    let mut ban_counts: Vec<usize> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(35);

    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        let from_admin = event.server_id == "admin";
        match &event.event {
            SessionEvent::Channels(list) if from_admin => {
                channels = list.iter().map(|c| (c.id, c.name.clone())).collect();
            }
            SessionEvent::Users(users) if from_admin => {
                let Some(them) = users.iter().find(|u| u.name == "tyre-kicker") else {
                    continue;
                };
                target = Some(them.session);
                let garage = channels
                    .iter()
                    .find(|(_, n)| *n == channel)
                    .map(|(id, _)| *id);

                if let (false, Some(to)) = (moved_them, garage) {
                    moved_them = true;
                    println!("moving them to {channel}");
                    manager
                        .send(
                            &admin_id,
                            C::MoveUser {
                                session: them.session,
                                channel_id: to,
                            },
                        )
                        .await
                        .expect("move");
                } else if moved_them && !saw_them_moved && Some(them.channel_id) == garage {
                    saw_them_moved = true;
                    println!("they are in {channel}; banning them");
                    banned = true;
                    manager
                        .send(
                            &admin_id,
                            C::BanUser {
                                session: them.session,
                                reason: "live test".into(),
                            },
                        )
                        .await
                        .expect("ban");
                    manager.send(&admin_id, C::RequestBans).await.expect("bans");
                }
            }
            SessionEvent::Bans(list) => {
                println!(
                    "ban list: {:?}",
                    list.iter().map(|b| b.name.clone()).collect::<Vec<_>>()
                );
                ban_counts.push(list.len());
                for b in list {
                    ban_names.push(b.name.clone());
                }
                // Lift everything this test put there, so the server is left
                // as it was found — and read the list once more to prove it.
                if ban_counts.len() == 1 && !list.is_empty() {
                    manager
                        .send(&admin_id, C::SetBans(Vec::new()))
                        .await
                        .expect("lift");
                }
            }
            _ => {}
        }
    }
    manager.shutdown_all().await;

    assert!(target.is_some(), "the other rider never appeared");
    assert!(
        saw_them_moved,
        "moving somebody between channels did not take"
    );
    assert!(banned, "never got as far as banning");
    assert!(
        !ban_counts.is_empty(),
        "the ban list was never answered after a ban"
    );
    assert_eq!(
        ban_counts.first(),
        Some(&1),
        "a ban did not appear in the list: {ban_names:?}"
    );
    assert_eq!(
        ban_counts.last(),
        Some(&0),
        "the ban was not lifted again, and the server is not as it was found"
    );
}

/// A channel's access list: read it, change it, read it back.
///
/// The riskiest write in the client, for the same reason as the ban list — the
/// protocol has no way to change one rule, so the list is replaced whole and a
/// rule dropped on the way through would be a permission silently revoked.
/// This adds one, confirms the server kept it, removes it again, and confirms
/// the list is back where it started.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live server AND MW_LIVE_SUPERUSER set to its password"]
async fn an_acl_survives_a_round_trip() {
    require_server!();
    let Ok(password) = std::env::var("MW_LIVE_SUPERUSER") else {
        eprintln!("MW_LIVE_SUPERUSER is not set; skipping");
        return;
    };
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live acl").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    let mut admin = ServerProfile::new("live", host, port, "SuperUser");
    admin.id = "admin".into();
    admin.password = Some(password);
    let id = manager.add(admin, silent_bridge()).expect("added");
    manager
        .send(&id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    use mumbleway_core::session::permissions;
    use mumbleway_core::session::{AclRule, SessionCommand as C};

    let mut seen: Vec<usize> = Vec::new();
    let mut groups_seen: Vec<String> = Vec::new();
    let mut step = 0;
    let mut added_was_there = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        match &event.event {
            SessionEvent::SelfSession(_) if step == 0 => {
                step = 1;
                manager.send(&id, C::RequestAcl(0)).await.expect("read acl");
            }
            SessionEvent::Acl(acl) => {
                let own: Vec<&AclRule> = acl.rules.iter().filter(|r| !r.inherited).collect();
                println!(
                    "acl of channel {}: {} rules ({} of them this channel's), {} groups, inherit={}",
                    acl.channel_id,
                    acl.rules.len(),
                    own.len(),
                    acl.groups.len(),
                    acl.inherit_acls
                );
                if step == 1 {
                    step = 2;
                    seen.push(own.len());
                    groups_seen = acl.groups.iter().map(|g| g.name.clone()).collect();

                    // Add a rule nobody else would write: deny Whisper to the
                    // "all" group. Harmless, and unmistakable on the way back.
                    let mut next = acl.clone();
                    next.rules.push(AclRule {
                        apply_here: true,
                        apply_subs: false,
                        inherited: false,
                        user_id: None,
                        group: Some("all".into()),
                        grant: 0,
                        deny: permissions::WHISPER,
                    });
                    manager.send(&id, C::SetAcl(next)).await.expect("write acl");
                } else if step == 2 {
                    step = 3;
                    seen.push(own.len());
                    added_was_there = acl.rules.iter().any(|r| {
                        !r.inherited
                            && r.group.as_deref() == Some("all")
                            && r.deny == permissions::WHISPER
                    });
                    // And take it away again, leaving the server as found.
                    let mut next = acl.clone();
                    next.rules.retain(|r| {
                        !(r.group.as_deref() == Some("all") && r.deny == permissions::WHISPER)
                    });
                    manager
                        .send(&id, C::SetAcl(next))
                        .await
                        .expect("restore acl");
                } else if step == 3 {
                    step = 4;
                    seen.push(own.len());
                }
            }
            SessionEvent::Refused { reason, kind } => {
                println!("server refused something: kind={kind} reason={reason:?}");
            }
            SessionEvent::Text { from, message } => {
                println!("text from {from}: {message}");
            }
            _ => {}
        }
    }
    manager.shutdown_all().await;

    println!("rule counts seen: {seen:?}");
    println!("groups: {groups_seen:?}");
    assert!(
        seen.len() >= 3,
        "the list was not read three times: {seen:?}"
    );
    assert!(
        added_was_there,
        "the rule that was written did not come back"
    );
    assert_eq!(
        seen[2], seen[0],
        "the server was not left as it was found: {seen:?}"
    );
    assert!(
        groups_seen.iter().any(|g| g == "admin"),
        "a default server defines an admin group: {groups_seen:?}"
    );
}

/// Everything an admin can now do, in one round trip on a real server.
///
/// Channel management, the registered-user list, registering and unregistering
/// somebody, priority speaker, and the privileged half of `UserStats`.
///
/// **The admin acts on a second rider, not on itself.** Murmur refuses any flag
/// change whose target is the SuperUser —
/// `if (pDstServerUser->iId == 0) { PERM_DENIED_TYPE(SuperUser); return; }` —
/// so an earlier version of this test granted priority speaker to SuperUser and
/// read the server's refusal as the client's failure.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live server AND MW_LIVE_SUPERUSER set to its password"]
async fn an_admin_can_run_the_server() {
    require_server!();
    let Ok(password) = std::env::var("MW_LIVE_SUPERUSER") else {
        eprintln!("MW_LIVE_SUPERUSER is not set; skipping");
        return;
    };
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live admin").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    let mut admin = ServerProfile::new("live", host.clone(), port, "SuperUser");
    admin.id = "admin".into();
    admin.password = Some(password);
    let admin_id = manager.add(admin, silent_bridge()).expect("added");
    manager
        .send(&admin_id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    let mut ordinary = ServerProfile::new("live", host, port, "ordinary-rider");
    ordinary.id = "ordinary".into();
    let ordinary_id = manager.add(ordinary, silent_bridge()).expect("added");
    manager
        .send(
            &ordinary_id,
            mumbleway_core::session::SessionCommand::Connect,
        )
        .await
        .expect("connect");

    use mumbleway_core::session::SessionCommand as C;
    const MADE: &str = "Live Test Channel";
    const RENAMED: &str = "Live Test Channel (renamed)";

    let mut step = 0;
    let mut saw_made = false;
    let mut saw_renamed = false;
    let mut saw_removed = false;
    let mut registered_counts: Vec<usize> = Vec::new();
    let mut registered_names: Vec<String> = Vec::new();
    let mut details = None;
    let mut priority_seen = false;
    let mut target: Option<u32> = None;
    let mut acted_on_target = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);

    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        let from_admin = event.server_id == "admin";
        match &event.event {
            SessionEvent::SelfSession(_) if from_admin && step == 0 => {
                step = 1;
                manager
                    .send(
                        &admin_id,
                        C::CreateChannel {
                            parent: 0,
                            name: MADE.into(),
                            description: "made by the live test".into(),
                            temporary: false,
                        },
                    )
                    .await
                    .expect("create");
            }
            SessionEvent::Channels(list) if from_admin => {
                let made = list.iter().find(|c| c.name == MADE);
                let renamed = list.iter().find(|c| c.name == RENAMED);
                if let (1, Some(c)) = (step, made) {
                    saw_made = true;
                    step = 2;
                    println!("created channel {} ({})", c.name, c.id);
                    manager
                        .send(
                            &admin_id,
                            C::EditChannel {
                                channel_id: c.id,
                                name: Some(RENAMED.into()),
                                description: None,
                            },
                        )
                        .await
                        .expect("edit");
                } else if let (2, Some(c)) = (step, renamed) {
                    saw_renamed = true;
                    step = 3;
                    println!("renamed to {}", c.name);
                    manager
                        .send(&admin_id, C::RemoveChannel(c.id))
                        .await
                        .expect("remove");
                } else if step == 3 && made.is_none() && renamed.is_none() {
                    saw_removed = true;
                    println!("and removed again");
                }
            }
            SessionEvent::Users(users) if from_admin => {
                if let Some(them) = users.iter().find(|u| u.name == "ordinary-rider") {
                    target = Some(them.session);
                    if them.priority_speaker {
                        priority_seen = true;
                    }
                }
                if let (Some(them), false) = (target, acted_on_target) {
                    acted_on_target = true;
                    manager
                        .send(&admin_id, C::RequestUserDetails(them))
                        .await
                        .expect("details");
                    manager
                        .send(
                            &admin_id,
                            C::SetPrioritySpeaker {
                                session: them,
                                priority: true,
                            },
                        )
                        .await
                        .expect("priority");
                    manager
                        .send(&admin_id, C::RegisterUser(them))
                        .await
                        .expect("register");
                    manager
                        .send(&admin_id, C::RequestRegistered)
                        .await
                        .expect("list");
                }
            }
            SessionEvent::Registered(list) => {
                println!(
                    "registered users: {:?}",
                    list.iter().map(|u| u.name.clone()).collect::<Vec<_>>()
                );
                registered_counts.push(list.len());
                for u in list {
                    registered_names.push(u.name.clone());
                }
                // Having seen them registered, take it away and look again —
                // removal is the half with no message of its own.
                if registered_counts.len() == 1 {
                    if let Some(them) = list.iter().find(|u| u.name == "ordinary-rider") {
                        manager
                            .send(&admin_id, C::UnregisterUsers(vec![them.user_id]))
                            .await
                            .expect("unregister");
                        manager
                            .send(&admin_id, C::RequestRegistered)
                            .await
                            .expect("list again");
                    }
                }
            }
            SessionEvent::UserDetails(d) => {
                println!(
                    "details: release={:?} os={:?} address={:?} strong_cert={}",
                    d.release, d.os, d.address, d.strong_certificate
                );
                details = Some(d.clone());
            }
            _ => {}
        }
    }
    manager.shutdown_all().await;

    assert!(saw_made, "the channel was never created");
    assert!(saw_renamed, "the rename never took");
    assert!(saw_removed, "the channel was not removed");
    assert!(priority_seen, "priority speaker never took effect");

    let details = details.expect("no user details came back");
    assert!(
        !details.release.is_empty(),
        "an admin asking about a client was told nothing about it"
    );
    assert!(
        !details.address.is_empty(),
        "the address is the privileged half; without it stats_only is still on"
    );

    println!("registered list sizes seen: {registered_counts:?}");
    assert!(
        registered_names.iter().any(|n| n == "ordinary-rider"),
        "registering somebody did not put them in the list: {registered_names:?}"
    );
    assert!(
        registered_counts.len() >= 2 && registered_counts.last() < registered_counts.first(),
        "unregistering did not take: {registered_counts:?}"
    );
}

/// A rider's picture, there and back.
///
/// The server stores the bytes against the account and broadcasts them to
/// everybody, so what this checks is the whole loop: sent as a `UserState`
/// texture, stored, returned — possibly as a hash that has to be fetched — and
/// turned back into the same bytes.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_picture_set_on_a_server_comes_back() {
    require_server!();
    let (host, port) = live_address().expect("MW_LIVE");
    let (tx, mut rx) = mpsc::channel(4096);
    let identity = Identity::generate("MumbleWay live test").expect("identity");
    let mut manager =
        SessionManager::new(identity, "MumbleWay 0.0-live", tx).with_app_version("0.0-live");

    let mut profile = ServerProfile::new("live", host, port, "face");
    profile.id = "face".into();
    let id = manager.add(profile, silent_bridge()).expect("added");
    manager
        .send(&id, mumbleway_core::session::SessionCommand::Connect)
        .await
        .expect("connect");

    // A one-pixel PNG: the smallest thing that is really an image.
    let png: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let mut sent = false;
    let mut back: Option<Vec<u8>> = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(16);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(left, rx.recv()).await else {
            break;
        };
        match &event.event {
            SessionEvent::SelfSession(_) if !sent => {
                sent = true;
                manager
                    .send(
                        &id,
                        mumbleway_core::session::SessionCommand::SetAvatar(png.clone()),
                    )
                    .await
                    .expect("set avatar");
            }
            SessionEvent::Avatar { image, .. } if !image.is_empty() => {
                back = Some(image.clone());
            }
            _ => {}
        }
    }
    manager.shutdown_all().await;

    println!(
        "picture came back as {:?} bytes",
        back.as_ref().map(|b| b.len())
    );
    assert!(sent, "never got far enough to send one");
    assert_eq!(
        back.as_deref(),
        Some(png.as_slice()),
        "what came back is not what was sent"
    );
}

/// The things a server tells a client about itself, and the two blobs it only
/// hands over when asked.
///
/// `MW_LIVE_DESCRIBED_CHANNEL` names a channel whose description is long
/// enough to travel as a hash — over 128 bytes — which is the case this is
/// really about: a short one arrives whole in the `ChannelState` and proves
/// nothing about `RequestBlob`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Mumble server; see the file header"]
async fn a_server_states_its_limits_and_hands_over_what_is_asked_for() {
    require_server!();
    let events = gather(&["reader"], 14).await;

    let limits: Vec<_> = events
        .iter()
        .filter_map(|e| match e.event {
            SessionEvent::Limits(l) => Some(l),
            _ => None,
        })
        .collect();
    println!("limits reported: {limits:?}");
    let limits = limits.last().expect("ServerConfig was never read");
    assert!(
        limits.message_length > 0 && limits.image_message_length > 0,
        "a default server states both; reading neither means ServerConfig is ignored"
    );
    // The arithmetic those figures drive, against real numbers.
    assert!(
        limits.image_fits(1_024),
        "a scaled avatar is a few kilobytes"
    );
    assert!(!limits.image_fits(limits.image_message_length as usize + 1));
    let long = "x".repeat(limits.message_length as usize + 50);
    assert_eq!(limits.fit_text(&long).len(), limits.message_length as usize);

    if let Ok(name) = std::env::var("MW_LIVE_DESCRIBED_CHANNEL") {
        // A description over 128 bytes arrives as a hash and has to be
        // fetched; what is checked is that it ends up as readable text.
        let described = events
            .iter()
            .filter_map(|e| match &e.event {
                SessionEvent::Channels(list) => list.iter().find(|c| c.name == name).cloned(),
                _ => None,
            })
            .rfind(|c| !c.description.is_empty());
        let described =
            described.unwrap_or_else(|| panic!("channel {name} never arrived with a description"));
        println!(
            "description of {name}: {} chars, starts {:?}",
            described.description.chars().count(),
            described.description.chars().take(40).collect::<String>()
        );
        assert!(
            described.description.len() > 128,
            "the long description was never fetched, so only short ones work"
        );
    }
}

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
