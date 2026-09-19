//! Menu entries a server adds to this client.
//!
//! A Mumble server — or a bot living on one — can register an action with
//! `ContextActionModify`, and the client is expected to show it in the menu for
//! a user, a channel, or the server, and to send back `ContextAction` when
//! somebody picks it. This is how a recording bot offers "start recording", how
//! a ride organiser offers "call everyone to this channel", and so on.
//!
//! **The text is the server's, and so is the meaning.** Nothing here knows what
//! an action does, and there is nothing to translate: an entry says what
//! whoever registered it chose to call it. That is also the reason the entries
//! are kept exactly as sent and never merged or rewritten — the identifier is
//! what goes back, and it has to go back unchanged.
//!
//! # What is guarded here
//!
//! A registration is an outsider putting an entry into this rider's interface,
//! so three things are bounded rather than trusted: how many entries a server
//! may add, how long the text may be, and what characters it may contain. A
//! server that registers a thousand actions, or one whose label is a screenful
//! of newlines, gets a menu that cannot be used — on a phone, at a junction.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Bit set by the server when the action belongs in the server's own menu.
pub const CONTEXT_SERVER: u32 = 0x01;
/// Bit for actions that target a channel.
pub const CONTEXT_CHANNEL: u32 = 0x02;
/// Bit for actions that target a user.
pub const CONTEXT_USER: u32 = 0x04;

/// Most actions one server may register.
///
/// Well past what any real server uses; low enough that a menu stays a menu.
pub const MAX_ACTIONS: usize = 32;

/// Longest label shown, in characters.
pub const MAX_LABEL_CHARS: usize = 64;

/// One entry a server asked this client to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAction {
    /// The identifier sent back when it is picked. The server's own string.
    pub action: String,
    /// What to call it in the menu.
    pub label: String,
    /// Where it belongs: any of [`CONTEXT_SERVER`], [`CONTEXT_CHANNEL`],
    /// [`CONTEXT_USER`], or several at once.
    pub context: u32,
}

impl ContextAction {
    pub fn for_user(&self) -> bool {
        self.context & CONTEXT_USER != 0
    }

    pub fn for_channel(&self) -> bool {
        self.context & CONTEXT_CHANNEL != 0
    }

    pub fn for_server(&self) -> bool {
        self.context & CONTEXT_SERVER != 0
    }
}

/// Every action a server has registered on this connection.
///
/// A `BTreeMap` keyed by identifier: registrations arrive in whatever order the
/// server sends them and may be replaced, and a menu whose entries move about
/// between openings is one people mis-tap.
#[derive(Debug, Default)]
pub struct ContextActions {
    by_id: BTreeMap<String, ContextAction>,
}

impl ContextActions {
    /// Takes a registration, returning whether anything changed.
    ///
    /// Refused when the label is empty after cleaning — an unnamed entry is an
    /// entry nobody can make sense of — or when the server has already
    /// registered [`MAX_ACTIONS`]. Replacing one it already registered is
    /// always allowed, since that is how a bot renames its own entry.
    pub fn add(&mut self, action: &str, label: &str, context: u32) -> bool {
        let id = action.trim();
        if id.is_empty() {
            return false;
        }
        let label = clean_label(label);
        if label.is_empty() {
            return false;
        }
        // No context at all means there is nowhere to put it.
        if context & (CONTEXT_SERVER | CONTEXT_CHANNEL | CONTEXT_USER) == 0 {
            return false;
        }
        let replacing = self.by_id.contains_key(id);
        if !replacing && self.by_id.len() >= MAX_ACTIONS {
            return false;
        }
        let entry = ContextAction {
            action: id.to_string(),
            label,
            context,
        };
        if self.by_id.get(id) == Some(&entry) {
            return false;
        }
        self.by_id.insert(id.to_string(), entry);
        true
    }

    /// Removes one, returning whether it was there.
    pub fn remove(&mut self, action: &str) -> bool {
        self.by_id.remove(action.trim()).is_some()
    }

    /// Every entry, in a stable order.
    pub fn all(&self) -> Vec<ContextAction> {
        self.by_id.values().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// Makes a label safe to put in a menu.
///
/// Control characters go — a newline in a menu entry is a menu entry that
/// spans the screen — runs of whitespace collapse, and the result is cut to
/// [`MAX_LABEL_CHARS`] by characters rather than bytes.
pub fn clean_label(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_LABEL_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_is_offered_where_the_server_said() {
        let mut a = ContextActions::default();
        assert!(a.add("rec", "Start recording", CONTEXT_USER | CONTEXT_CHANNEL));

        let entry = &a.all()[0];
        assert!(entry.for_user());
        assert!(entry.for_channel());
        assert!(!entry.for_server());
    }

    #[test]
    fn the_identifier_goes_back_exactly_as_it_came() {
        // It is the server's own key, not something to tidy up: an action
        // renamed on the way through is an action the server never hears about.
        let mut a = ContextActions::default();
        a.add("bot/Record.Start", "Record", CONTEXT_USER);
        assert_eq!(a.all()[0].action, "bot/Record.Start");
    }

    #[test]
    fn removing_takes_it_out_of_the_menu() {
        let mut a = ContextActions::default();
        a.add("rec", "Record", CONTEXT_USER);
        assert!(a.remove("rec"));
        assert!(a.is_empty());
        assert!(!a.remove("rec"), "and removing it twice is not a change");
    }

    #[test]
    fn a_bot_may_rename_its_own_entry() {
        let mut a = ContextActions::default();
        a.add("rec", "Start recording", CONTEXT_USER);
        assert!(a.add("rec", "Stop recording", CONTEXT_USER));
        assert_eq!(a.all().len(), 1);
        assert_eq!(a.all()[0].label, "Stop recording");
    }

    #[test]
    fn the_same_registration_twice_is_not_news() {
        // Servers re-send these, and rebuilding the menu for an identical
        // entry would be a rebuild for nothing.
        let mut a = ContextActions::default();
        assert!(a.add("rec", "Record", CONTEXT_USER));
        assert!(!a.add("rec", "Record", CONTEXT_USER));
    }

    #[test]
    fn an_unnamed_or_unplaced_action_is_refused() {
        let mut a = ContextActions::default();
        assert!(!a.add("rec", "   ", CONTEXT_USER), "nothing to show");
        assert!(!a.add("", "Record", CONTEXT_USER), "nothing to send back");
        assert!(!a.add("rec", "Record", 0), "nowhere to put it");
        assert!(a.is_empty());
    }

    #[test]
    fn a_label_cannot_take_over_the_screen() {
        // This is somebody else's text in this rider's menu, read at a
        // junction. Newlines and length are both theirs to choose and neither
        // is theirs to spend.
        assert_eq!(clean_label("Start\n\nrecording"), "Start recording");
        assert_eq!(
            clean_label(&"x".repeat(500)).chars().count(),
            MAX_LABEL_CHARS
        );
        assert_eq!(
            clean_label("я".repeat(500).as_str()).chars().count(),
            MAX_LABEL_CHARS
        );
    }

    #[test]
    fn a_server_cannot_register_a_thousand_entries() {
        let mut a = ContextActions::default();
        for i in 0..MAX_ACTIONS + 20 {
            a.add(&format!("a{i}"), "Action", CONTEXT_USER);
        }
        assert_eq!(a.all().len(), MAX_ACTIONS);
        // And the ones it did register still work, including renaming them.
        assert!(a.add("a0", "Renamed", CONTEXT_USER));
    }

    #[test]
    fn the_order_does_not_shift_between_openings() {
        // Menus are tapped from memory, at speed, sometimes wearing gloves.
        let mut a = ContextActions::default();
        a.add("zulu", "Z", CONTEXT_USER);
        a.add("alpha", "A", CONTEXT_USER);
        a.add("mike", "M", CONTEXT_USER);

        let first: Vec<String> = a.all().into_iter().map(|e| e.action).collect();
        let again: Vec<String> = a.all().into_iter().map(|e| e.action).collect();
        assert_eq!(first, again);
        assert_eq!(first, vec!["alpha", "mike", "zulu"]);
    }
}
