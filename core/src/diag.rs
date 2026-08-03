//! The app's own log: a ring of recent events, and the plumbing that makes
//! `tracing` calls land in it.
//!
//! Motivated by the fact that a rider cannot attach a debugger to a phone in a
//! helmet. When something goes wrong on a device we do not have, the only
//! evidence is what the app itself recorded, so the log has to be readable from
//! inside the app and quotable back to us.
//!
//! Built on `tracing` rather than a bespoke logging call so that the four
//! `tracing::error!` sites that already existed start being seen — until now
//! nothing installed a subscriber, so messages like "cannot create Opus
//! encoder" were formatted and then dropped on the floor.
//!
//! Only events are captured, not spans. A subscriber that ignores spans is
//! about forty lines and costs no dependency; one that honours them means
//! pulling in `tracing-subscriber` to serve a feature this log does not have.
//! If spans are ever wanted here, that trade is the thing to revisit.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use tracing::field::{Field, Visit};
use tracing::{Event, Metadata, Subscriber};

/// How many entries to keep.
///
/// Enough to cover a connect, a failure and a retry or two at the rate these
/// are emitted, which is the span that usually contains the answer. Older
/// entries fall off the front: a log that stops recording once full loses the
/// part nearest the problem, which is the opposite of useful.
const CAPACITY: usize = 1000;

/// Longest message kept, in bytes.
///
/// Guards the ring against a single enormous formatted value — a whole roster,
/// say — quietly costing more than every other entry together.
const MAX_MESSAGE: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn of(meta: &Metadata<'_>) -> Self {
        match *meta.level() {
            tracing::Level::TRACE => LogLevel::Trace,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Monotonic, so a reader can ask for "everything after what I have" without
    /// timestamps having to be unique or even ordered.
    pub seq: u64,
    /// Milliseconds since the Unix epoch.
    pub at_ms: u64,
    pub level: LogLevel,
    /// The module that emitted it, trimmed to the interesting tail.
    pub target: String,
    pub message: String,
}

static NEXT_SEQ: AtomicU64 = AtomicU64::new(1);
static LOG: Mutex<VecDeque<LogEntry>> = Mutex::new(VecDeque::new());

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Records one entry directly, for callers that are not going through `tracing`.
pub fn record(level: LogLevel, target: &str, message: impl Into<String>) {
    let mut message = message.into();
    if message.len() > MAX_MESSAGE {
        // On a character boundary: truncating a UTF-8 string mid-sequence
        // panics, and a log has no business crashing the thing it observes.
        let mut cut = MAX_MESSAGE;
        while cut > 0 && !message.is_char_boundary(cut) {
            cut -= 1;
        }
        message.truncate(cut);
        message.push('…');
    }

    let entry = LogEntry {
        seq: NEXT_SEQ.fetch_add(1, Ordering::Relaxed),
        at_ms: now_ms(),
        level,
        target: short_target(target),
        message,
    };

    let mut log = LOG.lock();
    if log.len() >= CAPACITY {
        log.pop_front();
    }
    log.push_back(entry);
}

/// `mumbleway_core::session::manager` is mostly prefix; the tail is the part
/// that tells a reader which subsystem spoke.
fn short_target(target: &str) -> String {
    target.rsplit("::").next().unwrap_or(target).to_string()
}

/// Everything recorded after `seq`, oldest first.
///
/// The caller passes back the highest `seq` it has already seen, which makes
/// polling cheap and idempotent — a dropped or repeated poll cannot duplicate
/// or lose a line.
pub fn since(seq: u64) -> Vec<LogEntry> {
    LOG.lock().iter().filter(|e| e.seq > seq).cloned().collect()
}

/// The whole ring, oldest first.
pub fn snapshot() -> Vec<LogEntry> {
    LOG.lock().iter().cloned().collect()
}

pub fn clear() {
    LOG.lock().clear();
}

/// Sends `tracing` events to the ring.
///
/// Spans are accepted and discarded: the trait requires the methods, and
/// answering them with a constant id is correct for a subscriber that never
/// looks at span data.
struct RingSubscriber;

const SPAN_ID: u64 = 1;

impl Subscriber for RingSubscriber {
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        // Trace is where per-packet logging lives; keeping it would push the
        // useful entries out of a 1000-entry ring within seconds.
        *meta.level() <= tracing::Level::DEBUG
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::Id {
        tracing::Id::from_u64(SPAN_ID)
    }

    fn record(&self, _: &tracing::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::Id, _: &tracing::Id) {}
    fn enter(&self, _: &tracing::Id) {}
    fn exit(&self, _: &tracing::Id) {}

    fn event(&self, event: &Event<'_>) {
        let meta = event.metadata();
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        record(LogLevel::of(meta), meta.target(), visitor.finish());
    }
}

/// Flattens an event's fields into one line.
///
/// `message` is the format string every `tracing::info!("…")` produces and is
/// kept bare; anything else is a structured field and is named, so a line reads
/// as prose with detail appended rather than as a bag of keys.
#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: String,
}

impl MessageVisitor {
    fn finish(self) -> String {
        match (self.message.is_empty(), self.fields.is_empty()) {
            (true, _) => self.fields,
            (false, true) => self.message,
            (false, false) => format!("{} ({})", self.message, self.fields),
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
            return;
        }
        if !self.fields.is_empty() {
            self.fields.push_str(", ");
        }
        self.fields
            .push_str(&format!("{}={:?}", field.name(), value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
            return;
        }
        if !self.fields.is_empty() {
            self.fields.push_str(", ");
        }
        self.fields.push_str(&format!("{}={}", field.name(), value));
    }
}

/// Starts capturing `tracing` events.
///
/// Safe to call more than once and from anywhere: setting the global default
/// can only succeed once, and a second attempt is a no-op rather than an error
/// worth reporting — tests in the same process race for it legitimately.
pub fn install() {
    let _ = tracing::subscriber::set_global_default(RingSubscriber);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ring is process-wide, so tests that read it cannot run concurrently
    /// with each other. One test, several assertions.
    #[test]
    fn records_trims_and_scrolls() {
        clear();

        record(LogLevel::Info, "mumbleway_core::session::manager", "hello");
        let entries = snapshot();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "hello");
        // Only the tail of the module path survives.
        assert_eq!(entries[0].target, "manager");
        assert!(entries[0].at_ms > 0);

        // `since` is exclusive, so asking again with the last seq yields nothing.
        let last = entries[0].seq;
        assert!(since(last).is_empty());
        record(LogLevel::Warn, "audio", "second");
        let fresh = since(last);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].message, "second");

        // Oversized messages are cut, and cut somewhere legal.
        clear();
        record(LogLevel::Info, "t", "é".repeat(MAX_MESSAGE));
        let long = snapshot();
        assert!(long[0].message.len() <= MAX_MESSAGE + '…'.len_utf8());
        assert!(long[0].message.ends_with('…'));

        // Past capacity the oldest go, not the newest.
        clear();
        for i in 0..CAPACITY + 50 {
            record(LogLevel::Debug, "t", format!("{i}"));
        }
        let full = snapshot();
        assert_eq!(full.len(), CAPACITY);
        assert_eq!(full[0].message, "50");
        assert_eq!(full[CAPACITY - 1].message, format!("{}", CAPACITY + 49));
    }
}
