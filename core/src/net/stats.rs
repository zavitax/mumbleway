//! Process-wide traffic counters, for the diagnostics panel.
//!
//! Statics rather than a handle threaded through every transport. These are
//! monotonic counters that only ever get added to and read for display, so the
//! usual objections to global state — ordering, ownership, testability — do not
//! bite, and the alternative is plumbing a stats handle through the control
//! reader, the control writer, the voice socket and every call site that owns
//! one, to serve a panel.
//!
//! Counted as close to the wire as possible: what the socket actually carried,
//! not what the layers above meant to send. A framing bug that doubles every
//! header is exactly the sort of thing this should be able to show.

use std::sync::atomic::{AtomicU64, Ordering};

static BYTES_IN: AtomicU64 = AtomicU64::new(0);
static BYTES_OUT: AtomicU64 = AtomicU64::new(0);
static VOICE_IN: AtomicU64 = AtomicU64::new(0);
static VOICE_OUT: AtomicU64 = AtomicU64::new(0);

pub fn note_bytes_in(n: usize) {
    BYTES_IN.fetch_add(n as u64, Ordering::Relaxed);
}

pub fn note_bytes_out(n: usize) {
    BYTES_OUT.fetch_add(n as u64, Ordering::Relaxed);
}

pub fn note_voice_in() {
    VOICE_IN.fetch_add(1, Ordering::Relaxed);
}

pub fn note_voice_out() {
    VOICE_OUT.fetch_add(1, Ordering::Relaxed);
}

/// `(bytes in, bytes out, voice packets in, voice packets out)` since start.
///
/// Cumulative on purpose: a rate depends on the interval it was measured over,
/// and only the caller knows how long it waited between reads.
pub fn snapshot() -> (u64, u64, u64, u64) {
    (
        BYTES_IN.load(Ordering::Relaxed),
        BYTES_OUT.load(Ordering::Relaxed),
        VOICE_IN.load(Ordering::Relaxed),
        VOICE_OUT.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate_and_are_read_together() {
        let (b0, o0, i0, u0) = snapshot();
        note_bytes_in(120);
        note_bytes_out(40);
        note_voice_in();
        note_voice_out();
        note_voice_out();
        let (b1, o1, i1, u1) = snapshot();

        // Deltas rather than absolutes: these are process-wide, so another
        // test running alongside may have moved them too.
        assert_eq!(b1 - b0, 120);
        assert_eq!(o1 - o0, 40);
        assert_eq!(i1 - i0, 1);
        assert_eq!(u1 - u0, 2);
    }
}
