//! Runs several server sessions at once.
//!
//! The dual-server feature is just "more than one slot": each session owns its
//! own socket, cipher state and reconnect policy, and the manager fans commands
//! out and aggregates events. Nothing about a session knows it has siblings, so
//! going from one to two servers costs no extra protocol logic.

use std::collections::HashMap;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{CoreError, Result};
use crate::net::tls::Identity;
use crate::session::{
    AudioBridge, BackoffPolicy, ConnectionState, ServerProfile, Session, SessionCommand,
    SessionConfig, SessionEvent,
};

/// How many servers may be connected simultaneously.
///
/// Two is the documented feature; the limit exists because every extra session
/// adds a full audio decode path and the mixer has to stay real-time.
pub const MAX_CONCURRENT_SESSIONS: usize = 2;

/// An event tagged with the server that produced it.
#[derive(Debug, Clone)]
pub struct TaggedEvent {
    pub server_id: String,
    pub event: SessionEvent,
}

struct Slot {
    profile: ServerProfile,
    commands: mpsc::Sender<SessionCommand>,
    state: ConnectionState,
    task: JoinHandle<()>,
}

/// Owns every active session.
pub struct SessionManager {
    slots: HashMap<String, Slot>,
    identity: Identity,
    client_name: String,
    events_out: mpsc::Sender<TaggedEvent>,
    max_sessions: usize,
}

impl SessionManager {
    pub fn new(
        identity: Identity,
        client_name: impl Into<String>,
        events_out: mpsc::Sender<TaggedEvent>,
    ) -> Self {
        Self {
            slots: HashMap::new(),
            identity,
            client_name: client_name.into(),
            events_out,
            max_sessions: MAX_CONCURRENT_SESSIONS,
        }
    }

    /// Overrides the concurrency limit (used by tests).
    pub fn with_max_sessions(mut self, n: usize) -> Self {
        self.max_sessions = n.max(1);
        self
    }

    pub fn server_ids(&self) -> Vec<String> {
        let mut v: Vec<_> = self.slots.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn state_of(&self, server_id: &str) -> Option<&ConnectionState> {
        self.slots.get(server_id).map(|s| &s.state)
    }

    /// Records a state change reported by a session.
    pub fn note_state(&mut self, server_id: &str, state: ConnectionState) {
        if let Some(slot) = self.slots.get_mut(server_id) {
            slot.state = state;
        }
    }

    /// Adds a session and starts its task. The session stays idle until it is
    /// sent [`SessionCommand::Connect`].
    ///
    /// `audio` wires this session to the audio engine.
    pub fn add(&mut self, profile: ServerProfile, audio: AudioBridge) -> Result<String> {
        if self.slots.contains_key(&profile.id) {
            return Err(CoreError::Other(format!(
                "already connected to {}",
                profile.name
            )));
        }
        if self.slots.len() >= self.max_sessions {
            return Err(CoreError::Other(format!(
                "at most {} servers can be connected at once",
                self.max_sessions
            )));
        }

        let id = profile.id.clone();
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (ev_tx, mut ev_rx) = mpsc::channel(256);

        let config = SessionConfig {
            profile: profile.clone(),
            identity: self.identity.clone(),
            client_name: self.client_name.clone(),
            backoff: BackoffPolicy::default(),
        };

        let session = Session::new(config, ev_tx, cmd_rx, audio);
        let task = tokio::spawn(session.run());

        // Re-tag this session's events onto the shared stream.
        let out = self.events_out.clone();
        let tag = id.clone();
        tokio::spawn(async move {
            while let Some(event) = ev_rx.recv().await {
                if out
                    .send(TaggedEvent {
                        server_id: tag.clone(),
                        event,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        self.slots.insert(
            id.clone(),
            Slot {
                profile,
                commands: cmd_tx,
                state: ConnectionState::Idle,
                task,
            },
        );
        Ok(id)
    }

    /// Sends a command to one session.
    pub async fn send(&self, server_id: &str, cmd: SessionCommand) -> Result<()> {
        let slot = self
            .slots
            .get(server_id)
            .ok_or_else(|| CoreError::Other(format!("unknown server {server_id}")))?;
        slot.commands
            .send(cmd)
            .await
            .map_err(|_| CoreError::Other("session is no longer running".into()))
    }

    /// Sends a command to every session (used for push-to-talk across servers).
    pub async fn broadcast(&self, cmd: SessionCommand) {
        for slot in self.slots.values() {
            let _ = slot.commands.send(cmd.clone()).await;
        }
    }

    /// Stops and removes one session.
    pub async fn remove(&mut self, server_id: &str) -> Result<()> {
        if let Some(slot) = self.slots.remove(server_id) {
            let _ = slot.commands.send(SessionCommand::Shutdown).await;
            slot.task.abort();
            Ok(())
        } else {
            Err(CoreError::Other(format!("unknown server {server_id}")))
        }
    }

    /// Stops everything.
    pub async fn shutdown_all(&mut self) {
        for (_, slot) in self.slots.drain() {
            let _ = slot.commands.send(SessionCommand::Shutdown).await;
            slot.task.abort();
        }
    }

    pub fn profile(&self, server_id: &str) -> Option<&ServerProfile> {
        self.slots.get(server_id).map(|s| &s.profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn bridge() -> AudioBridge {
        let (_tx, rx) = mpsc::channel(8);
        let (itx, _irx) = mpsc::channel(8);
        AudioBridge {
            outgoing: rx,
            incoming: itx,
        }
    }

    fn manager() -> (SessionManager, mpsc::Receiver<TaggedEvent>) {
        let (tx, rx) = mpsc::channel(64);
        let id = Identity::generate("test").unwrap();
        (SessionManager::new(id, "test", tx), rx)
    }

    #[tokio::test]
    async fn supports_two_simultaneous_servers() {
        let (mut m, _rx) = manager();
        // Point at ports with nothing listening; we are testing slot management,
        // not connectivity.
        let a = ServerProfile::new("A", "127.0.0.1", 1, "user");
        let b = ServerProfile::new("B", "127.0.0.1", 2, "user");

        assert!(m.add(a, bridge()).is_ok());
        assert!(m.add(b, bridge()).is_ok());
        assert_eq!(m.len(), 2, "the bonus feature is two concurrent servers");

        m.shutdown_all().await;
        assert!(m.is_empty());
    }

    #[tokio::test]
    async fn refuses_a_third_server() {
        let (mut m, _rx) = manager();
        m.add(ServerProfile::new("A", "127.0.0.1", 1, "u"), bridge())
            .unwrap();
        m.add(ServerProfile::new("B", "127.0.0.1", 2, "u"), bridge())
            .unwrap();
        let third = m.add(ServerProfile::new("C", "127.0.0.1", 3, "u"), bridge());
        assert!(third.is_err(), "must enforce the concurrency limit");
        m.shutdown_all().await;
    }

    #[tokio::test]
    async fn refuses_duplicate_servers() {
        let (mut m, _rx) = manager();
        let p = ServerProfile::new("A", "127.0.0.1", 1, "u");
        m.add(p.clone(), bridge()).unwrap();
        assert!(
            m.add(p, bridge()).is_err(),
            "the same host:port must not be added twice"
        );
        m.shutdown_all().await;
    }

    #[tokio::test]
    async fn profiles_get_stable_ids_from_host_and_port() {
        let a = ServerProfile::new("Name One", "mumble.example.com", 64738, "u");
        let b = ServerProfile::new("Name Two", "mumble.example.com", 64738, "u");
        assert_eq!(
            a.id, b.id,
            "id must key off the connection tuple, not the label"
        );

        let c = ServerProfile::new("Name One", "mumble.example.com", 64739, "u");
        assert_ne!(a.id, c.id, "a different port is a different server");
    }

    #[tokio::test]
    async fn commands_to_unknown_servers_error() {
        let (m, _rx) = manager();
        assert!(m.send("nope", SessionCommand::Connect).await.is_err());
    }

    #[tokio::test]
    async fn removing_a_session_frees_its_slot() {
        let (mut m, _rx) = manager();
        let id = m
            .add(ServerProfile::new("A", "127.0.0.1", 1, "u"), bridge())
            .unwrap();
        m.add(ServerProfile::new("B", "127.0.0.1", 2, "u"), bridge())
            .unwrap();
        assert!(m
            .add(ServerProfile::new("C", "127.0.0.1", 3, "u"), bridge())
            .is_err());

        m.remove(&id).await.unwrap();
        assert_eq!(m.len(), 1);
        // The freed slot can now be reused.
        assert!(m
            .add(ServerProfile::new("C", "127.0.0.1", 3, "u"), bridge())
            .is_ok());
        m.shutdown_all().await;
    }
}
