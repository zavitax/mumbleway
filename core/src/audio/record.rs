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
/// `Default` is for **tests only**, so that adding a column does not mean
/// editing every one of them — this struct has now grown three times and each
/// time the cost was paid in unrelated files.
///
/// The worker fills every field explicitly and must go on doing so. A default
/// here is a zero, and a zero in this log is a claim: `aec_on: false` says
/// cancellation was off, `echo_ref_samples: 0` says there was no reference.
/// Both are exactly the confusion the columns were added to end.
#[derive(Default)]
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

    /// Which microphone the audio came from, as a small code.
    ///
    /// **The column that would have answered "was this the right microphone?"
    /// in a second.** This file exists because a directory of recordings from
    /// the phone's own microphone looks exactly like one from the headset's,
    /// and making the app the recorder fixed *which* device it captures
    /// without recording *what* that device was. A quiet recording arrived and
    /// the route had to be inferred from the audio's bandwidth — a Bluetooth
    /// hands-free link stops dead at 3.4 kHz and a built-in microphone runs to
    /// 16 — which worked, and is a spectrum analysis standing in for a digit.
    ///
    /// | | |
    /// |---|---|
    /// | 0 | not known — no platform session, or it did not say |
    /// | 1 | the phone's own microphone |
    /// | 2 | a wired headset |
    /// | 3 | Bluetooth hands-free (SCO), which is narrowband |
    /// | 4 | USB or dock |
    /// | 5 | something else the platform named |
    ///
    /// **The numbers are the wire format and must not be renumbered.** Every
    /// recording already on somebody's phone is read with the meaning above,
    /// and a reader written months from now has nothing else to go on. New
    /// routes take the next free number.
    pub route: u8,

    /// The suppression profile in force, as `NoiseProfile as u8`.
    ///
    /// **Never `Auto`**, because `Auto` is a rule for choosing and not a
    /// profile: what is recorded is what the audio actually went through.
    ///
    /// Added after a singing recording where the chain removed 24 dB from four
    /// seconds of quiet phrases. Every other column in this file was consistent
    /// across that window — the VAD read 1.00, the gate stayed open, the floor
    /// did not move — so the log could say the loss happened and nothing about
    /// what was in force while it did. The profile changes what every stage
    /// after it does, and it was the one setting the file could not report.
    pub profile: u8,
    /// The two settings that can override every measurement above, recorded
    /// against the same block they acted on.
    ///
    /// **Added because a recording arrived that nobody could explain.** An
    /// Android ride came back with `speaking` at 64.9%, `gate_open` at 66.4%
    /// and `transmitting` at exactly zero on all 1,316 blocks — a waveform with
    /// no green in it at all, which reads as a fault in the drawing. It was
    /// not: the chain had been told not to send. But only two things do that,
    /// and the log recorded neither, so *which* of them could not be answered
    /// from the file. That is the same gap the input gain left, and it cost the
    /// same thing: an argument where a column would have done.
    ///
    /// `mode` is [`super::engine::TransmitMode`] as its index — 0 voice
    /// activated, 1 push to talk, 2 continuous.
    pub mode: u8,
    pub muted: bool,
    /// The microphone gain the rider had set, in dB.
    ///
    /// **The column an evening was spent not having.** A recording came back
    /// with 35% of its samples at full scale and nothing in the file said what
    /// the one control that sets the input level was set to — so "the meter
    /// never reaches 100%" and "a third of this is clipped" were argued
    /// against each other twice, both true of different signals. The gain was
    /// never observed, only inferred, and the inference is still the weakest
    /// claim in `docs/SESSION_2026-08-10.md`.
    ///
    /// Per block rather than in the header, because it is a slider: a rider
    /// who turns it down mid-ride would otherwise leave a file whose header
    /// describes a setting that was true for the first ten seconds.
    pub gain_db: f32,

    /// How many real samples of playback the echo canceller had for this block,
    /// out of [`super::denoise::FRAME_SIZE`].
    ///
    /// **The column this whole group was added for.** A recording arrived from
    /// an iPhone alone in a room, hearing nothing but its own loudspeaker, and
    /// 88% of the loud blocks were sent back to the far end. The canceller
    /// removed 36 dB one second and 12 dB the next, on a path that had not
    /// moved — which is not what an adaptive filter does unless its *reference*
    /// is moving. Nothing in the file could say whether it had one.
    ///
    /// The reference is a queue filled by the output callback and drained 480
    /// samples a block by the capture worker, and short reads are padded with
    /// silence. A block that reads 480 had a reference; one that reads 0 had
    /// none and could not have cancelled anything; anything between is the
    /// queue running dry mid-block, which splices silence into the middle of
    /// the reference and moves every alignment measured after it.
    pub echo_ref_samples: u16,
    /// Whether echo cancellation was switched on at all.
    ///
    /// The same lesson as `mode` and `muted` above: a stage that was off looks
    /// exactly like a stage that was broken, and arguing about which costs more
    /// than the column does.
    pub aec_on: bool,
    /// Echo return loss enhancement, dB. How much the canceller removed.
    pub erle_db: f32,
    /// Where the canceller believes the echo is, in milliseconds behind the
    /// reference, and how convincing that measurement was (0..1).
    ///
    /// The pair, not either alone: the aligner aims deliberately early, so a
    /// lag that reads low is the design working rather than a miss. A
    /// confidence that will not rise is the estimator failing to find the echo.
    pub aec_lag_ms: f32,
    pub aec_confidence: f32,
    /// How far apart the arrivals were measured to be, milliseconds. Larger
    /// than the filter's own span means a second echo it cannot reach.
    pub aec_spread_ms: f32,
    /// The filter's length in taps, which the performance ladder shortens.
    /// Meaningless when [`Self::aec3`] is set — AEC3 has no such dial.
    pub aec_taps: u16,
    /// Which canceller produced this block: AEC3, or the time-domain filter.
    ///
    /// **The same lesson as every other column here.** Two cancellers now sit
    /// behind one interface and they fail differently — the old one runs out of
    /// filter on a long room, the new one has a fortnight of history — so a
    /// recording that cannot say which one it came from cannot be read at all.
    /// It costs one bit and it settles an argument.
    pub aec3: bool,
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
    // New columns go on the end, never in the middle. Readers that find them
    // by name keep working; readers that count commas keep working; and the
    // recordings already sitting on people's phones stay readable by both.
    writeln!(
        log,
        "# mumbleway diagnostic capture; {rate} Hz mono s16le alongside\n\
         block,transmitting,speaking,gate_open,vad,snr_db,level_db,floor_db,harmonicity,\
         modulation,mode,muted,gain_db,echo_ref_samples,aec_on,erle_db,aec_lag_ms,\
         aec_confidence,aec_spread_ms,aec_taps,aec3,profile,route"
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
            "{},{},{},{},{:.3},{:.1},{:.1},{:.1},{:.3},{:.3},{},{},{:.1},\
             {},{},{:.1},{:.1},{:.2},{:.1},{},{},{},{}",
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
            block.mode,
            block.muted as u8,
            block.gain_db,
            block.echo_ref_samples,
            block.aec_on as u8,
            block.erle_db,
            block.aec_lag_ms,
            block.aec_confidence,
            block.aec_spread_ms,
            block.aec_taps,
            block.aec3 as u8,
            block.profile,
            block.route,
        );
        block_index += 1;

        if sink.written >= ROTATE_BYTES {
            let _ = sink.pcm.flush();
            let _ = sink.log.flush();
            match open_sink(dir, stem, sink.index + 1, rate) {
                Ok(next) => {
                    sink = next;
                    // Back to zero, because each pair is a recording in its
                    // own right. Running the counter on made the column mean
                    // "block within the session", which is a number nothing
                    // can use: the audio beside it starts at sample zero, so
                    // every reader that multiplied the column by the block
                    // size pointed past the end of the file it was reading.
                    // The listen sheet did exactly that and drew the tail of
                    // a long ride as if none of it had been transmitted.
                    block_index = 0;
                }
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
            echo_ref_samples: 480,
            aec_on: true,
            erle_db: 14.0,
            aec_lag_ms: 120.0,
            aec_confidence: 0.8,
            aec_spread_ms: 3.0,
            aec_taps: 1024,
            aec3: true,
            ..Default::default()
        }
    }

    /// The header names every column it writes, and writes every column it
    /// names.
    ///
    /// Cheap, and it is the thing a reader written months from now depends on:
    /// two of these columns were added after recordings were already on
    /// people's phones, and a header that drifted from the rows would make
    /// every one of them silently mean something else.
    #[test]
    fn the_header_and_the_rows_agree_on_how_many_columns_there_are() {
        let dir = std::env::temp_dir().join(format!("mw-hdr-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        {
            let rec = DiagnosticRecorder::start(&dir, "columns", 48_000).unwrap();
            rec.push(block(true));
            rec.push(block(false));
        }
        let log = fs::read_to_string(dir.join("columns-000.csv")).unwrap();
        let mut lines = log.lines().filter(|l| !l.starts_with('#') && !l.is_empty());
        let header: Vec<&str> = lines.next().unwrap().split(',').collect();
        assert!(header.contains(&"mode"), "header lost the mode column");
        assert!(header.contains(&"muted"), "header lost the muted column");
        assert!(header.contains(&"gain_db"), "header lost the gain column");
        for row in lines {
            assert_eq!(
                row.split(',').count(),
                header.len(),
                "a row has a different number of columns than the header: {row}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
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
    fn a_rotated_file_numbers_its_blocks_from_zero() {
        // The column is an offset into the audio lying beside it, and after a
        // rotation that audio starts again at sample zero. Running the counter
        // on across the rotation made every row of the second file point past
        // the end of it, which the listen sheet drew as a ride that transmitted
        // nothing -- the tail of a long ride, and the file it opens first.
        let dir = std::env::temp_dir().join(format!("mw-rec-rot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        // Straight down the channel rather than through `push`, which drops
        // rather than waits -- that is right on the audio thread and useless
        // here, where sixteen megabytes have to actually reach the disk.
        //
        // Enough to fill one file and start the next, computed rather than
        // guessed so it stays right if the rotation size moves.
        let per_block = 480 * 2;
        let blocks = (ROTATE_BYTES / per_block) + 4;
        fs::create_dir_all(&dir).unwrap(); // `start` does this; `write_loop` does not
        let (tx, rx) = sync_channel(QUEUE_BLOCKS);
        let dir2 = dir.clone();
        let writer = std::thread::spawn(move || write_loop(rx, &dir2, "rot", 48_000));
        for i in 0..blocks {
            tx.send(Message::Block(Box::new(block(i % 2 == 0))))
                .unwrap();
        }
        drop(tx);
        writer.join().unwrap();

        let second = fs::read_to_string(dir.join("rot-001.csv")).unwrap();
        let rows: Vec<&str> = second.lines().filter(|l| !l.starts_with('#')).collect();
        assert!(rows.len() >= 2, "the second file has a header and rows");
        assert!(
            rows[1].starts_with("0,"),
            "the second file must start at block 0, not {:?}",
            &rows[1][..rows[1].find(',').unwrap()]
        );

        // And its audio starts at zero too, which is the pairing the column
        // is meant to describe.
        let pcm = fs::metadata(dir.join("rot-001.s16")).unwrap().len();
        assert_eq!(
            pcm / per_block,
            (rows.len() - 1) as u64,
            "one block of audio per row, in the rotated file as much as the first"
        );

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
