//! Recording what the microphone gave us, and what the chain decided about it.
//!
//! Every measurement in this project was invalidated at once by finding that
//! the recordings behind it came from the phone's own microphone rather than
//! the headset's. Nothing in the analysis could have caught that: audio carries
//! no record of what captured it, and a directory of `.raw` files looks the
//! same either way.
//!
//! The fix is not to be more careful. It is to make the app the recorder, so
//! the audio is the chain's own input by construction and there is nothing left
//! to be wrong about.
//!
//! It writes two things per session:
//!
//! * **The capture**, as 16-bit PCM at the rate the chain runs at. Raw and
//!   headerless, because that is what the training pipeline reads and a WAV
//!   header is one more thing to get wrong on a phone.
//! * **A decision log**, one line per 10 ms block, holding what the chain
//!   concluded and why. This is the part that cannot be recovered afterwards:
//!   given the audio alone, "the gate was shut here" is an inference, and
//!   given this file it is a fact.
//!
//! # It must not touch the audio thread
//!
//! Opening files and writing to storage on a real-time path is how a capture
//! callback misses its deadline, and a missed deadline is a click in the audio
//! that will be blamed on the suppression. Blocks are handed to a writer thread
//! through a bounded channel and the audio thread never waits: if the queue is
//! full the block is dropped and counted. A diagnostic that degrades the thing
//! it is diagnosing is worse than no diagnostic.
//!
//! # Files are rotated
//!
//! At 48 kHz, 16-bit mono, a minute is 5.8 MB and an hour is 345. The rider has
//! to get these off the phone, and the intake bot in `tools/vad` is capped at
//! 20 MB by Telegram — so a file closes every few minutes and the next begins.
//! Chunks that arrive are usable; a single enormous file that cannot be sent is
//! not.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread;

/// Bytes per file before rotating. Below Telegram's 20 MB ceiling with room
/// for the decision log alongside it.
const ROTATE_BYTES: u64 = 16 * 1024 * 1024;

/// Blocks the writer may fall behind by before the audio thread starts
/// dropping them. Two seconds at 10 ms a block: long enough to cover a storage
/// stall, short enough that the memory is unremarkable.
const QUEUE_BLOCKS: usize = 200;

/// One block of audio and what the chain made of it.
pub struct Recorded {
    pub samples: Vec<f32>,
    /// The values worth keeping. Deliberately a fixed set rather than the whole
    /// of `BlockAnalysis`: this format has to stay readable by a script written
    /// months from now, and a struct that grows silently breaks every one of
    /// them.
    /// Whether this block's audio actually went on the wire.
    ///
    /// The one to cut a recording on, and not the same as [`Self::speaking`]:
    /// that is the instantaneous detector, before the hold and the fade, and
    /// before the mode has had its say. A rider muted, or in push-to-talk with
    /// their thumb off the button, produces blocks that are speech by every
    /// measure here and were sent to nobody.
    pub transmitting: bool,
    pub speaking: bool,
    pub gate_open: bool,
    pub vad: f32,
    pub snr_db: f32,
    pub level_db: f32,
    pub floor_db: f32,
    pub harmonicity: f32,
    pub modulation: f32,
}

enum Message {
    Block(Box<Recorded>),
    Stop,
}

/// Writes capture and decisions to disk, off the audio thread.
pub struct DiagnosticRecorder {
    tx: SyncSender<Message>,
    worker: Option<thread::JoinHandle<()>>,
    dropped: Arc<AtomicU64>,
    dir: PathBuf,
}

impl DiagnosticRecorder {
    /// Starts a session, writing into `dir`.
    ///
    /// `tag` names the files. It comes from the rider, so it is stripped to
    /// something a filesystem on any of five platforms will accept rather than
    /// trusted.
    pub fn start(dir: &Path, tag: &str, sample_rate: u32) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let safe: String = tag
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .take(40)
            .collect();
        let stem = if safe.trim_matches('-').is_empty() {
            "session".to_string()
        } else {
            safe
        };

        let (tx, rx) = sync_channel(QUEUE_BLOCKS);
        let dropped = Arc::new(AtomicU64::new(0));
        let dir_owned = dir.to_path_buf();
        let stem_owned = stem.clone();

        let worker = thread::Builder::new()
            .name("mumbleway-recorder".into())
            .spawn(move || write_loop(rx, &dir_owned, &stem_owned, sample_rate))?;

        Ok(Self {
            tx,
            worker: Some(worker),
            dropped,
            dir: dir.to_path_buf(),
        })
    }

    /// Hands over a block. Never blocks; never allocates beyond the block.
    pub fn push(&self, block: Recorded) {
        match self.tx.try_send(Message::Block(Box::new(block))) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                // Storage could not keep up. Counted rather than waited for:
                // the alternative is a click in the audio, and a rider would
                // report that as the suppression misbehaving.
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    /// Blocks the writer could not keep up with.
    pub fn dropped_blocks(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn directory(&self) -> &Path {
        &self.dir
    }
}

impl Drop for DiagnosticRecorder {
    fn drop(&mut self) {
        let _ = self.tx.send(Message::Stop);
        if let Some(worker) = self.worker.take() {
            // Joined rather than detached: the last file has to be flushed and
            // closed before anything offers to share it, or the rider sends a
            // truncated recording and we measure the truncation.
            let _ = worker.join();
        }
    }
}

struct Sink {
    pcm: BufWriter<File>,
    log: BufWriter<File>,
    written: u64,
    index: u32,
}

fn open_sink(dir: &Path, stem: &str, index: u32, rate: u32) -> std::io::Result<Sink> {
    let pcm_path = dir.join(format!("{stem}-{index:03}.s16"));
    let log_path = dir.join(format!("{stem}-{index:03}.csv"));
    let mut log = BufWriter::new(File::create(log_path)?);
    // A header, because the alternative is a column order remembered wrongly.
    writeln!(
        log,
        "# mumbleway diagnostic capture; {rate} Hz mono s16le alongside\n\
         block,transmitting,speaking,gate_open,vad,snr_db,level_db,floor_db,harmonicity,modulation"
    )?;
    Ok(Sink {
        pcm: BufWriter::new(File::create(pcm_path)?),
        log,
        written: 0,
        index,
    })
}

fn write_loop(rx: Receiver<Message>, dir: &Path, stem: &str, rate: u32) {
    let mut sink = match open_sink(dir, stem, 0, rate) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("diagnostic recording could not start: {e}");
            return;
        }
    };
    let mut block_index: u64 = 0;

    while let Ok(msg) = rx.recv() {
        let block = match msg {
            Message::Block(b) => b,
            Message::Stop => break,
        };

        let mut bytes = Vec::with_capacity(block.samples.len() * 2);
        for s in &block.samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        if sink.pcm.write_all(&bytes).is_err() {
            break;
        }
        sink.written += bytes.len() as u64;

        let _ = writeln!(
            sink.log,
            "{},{},{},{},{:.3},{:.1},{:.1},{:.1},{:.3},{:.3}",
            block_index,
            block.transmitting as u8,
            block.speaking as u8,
            block.gate_open as u8,
            block.vad,
            block.snr_db,
            block.level_db,
            block.floor_db,
            block.harmonicity,
            block.modulation,
        );
        block_index += 1;

        if sink.written >= ROTATE_BYTES {
            let _ = sink.pcm.flush();
            let _ = sink.log.flush();
            match open_sink(dir, stem, sink.index + 1, rate) {
                Ok(next) => sink = next,
                Err(e) => {
                    tracing::error!("could not rotate the diagnostic recording: {e}");
                    break;
                }
            }
        }
    }

    let _ = sink.pcm.flush();
    let _ = sink.log.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(speaking: bool) -> Recorded {
        Recorded {
            samples: vec![0.25; 480],
            transmitting: speaking,
            speaking,
            gate_open: speaking,
            vad: 0.9,
            snr_db: 12.0,
            level_db: -20.0,
            floor_db: -40.0,
            harmonicity: 0.5,
            modulation: 0.4,
        }
    }

    #[test]
    fn writes_audio_and_a_decision_for_every_block() {
        let dir = std::env::temp_dir().join(format!("mw-rec-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        {
            let rec = DiagnosticRecorder::start(&dir, "test run", 48_000).unwrap();
            for i in 0..10 {
                rec.push(block(i % 2 == 0));
            }
        } // dropped here, which flushes and joins

        let pcm = fs::read(dir.join("test-run-000.s16")).unwrap();
        assert_eq!(pcm.len(), 10 * 480 * 2, "one i16 per sample per block");

        let log = fs::read_to_string(dir.join("test-run-000.csv")).unwrap();
        let rows: Vec<&str> = log.lines().filter(|l| !l.starts_with('#')).collect();
        // A header row and one per block.
        assert_eq!(rows.len(), 11);
        assert!(rows[1].starts_with("0,1,1,"), "first block was speaking");
        assert!(rows[2].starts_with("1,0,0,"), "second was not");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hostile_tag_cannot_escape_the_directory() {
        // The tag comes from a text field a rider types into. It names files on
        // five platforms and must not be able to name one anywhere else.
        let dir = std::env::temp_dir().join(format!("mw-rec-esc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        {
            let rec = DiagnosticRecorder::start(&dir, "../../etc/passwd", 48_000).unwrap();
            rec.push(block(true));
        }
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().all(|n| !n.contains("..") && !n.contains('/')),
            "a path escaped into a filename: {names:?}"
        );
        assert_eq!(names.len(), 2, "one audio file and one log");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropping_blocks_is_counted_rather_than_waited_for() {
        // The audio thread must never block on storage. There is no way to
        // force a stall deterministically here, so this asserts the weaker but
        // still meaningful thing: pushing far more than the queue holds returns
        // promptly and the losses are visible rather than silent.
        let dir = std::env::temp_dir().join(format!("mw-rec-drop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let rec = DiagnosticRecorder::start(&dir, "drops", 48_000).unwrap();
        for _ in 0..(QUEUE_BLOCKS * 4) {
            rec.push(block(true));
        }
        // Whether anything was dropped depends on how fast the disk is, so the
        // count is not asserted -- only that asking is possible, which is what
        // makes a drop reportable instead of a mystery.
        let _ = rec.dropped_blocks();
        drop(rec);
        let _ = fs::remove_dir_all(&dir);
    }
}
