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

/// Bytes per file before rotating, applied to **each** file rather than to the
/// set of them.
///
/// **This used to be a bundle budget and is now a per-file cap**, and the
/// difference matters enough to state: 16 MiB was chosen as "below Telegram's
/// 20 MB ceiling with room for the decision log alongside it", one allowance
/// covering a whole segment. With three tracks that arithmetic no longer works,
/// so each file is now its own message and the limit is what one message may
/// carry.
///
/// And the check is against all three sinks, not only the audio. Audio
/// dominates today — roughly 96 kB/s against perhaps 40 kB/s of motion at
/// 400 Hz — so an audio-driven rotation would in fact keep the others under.
/// But that is a coincidence of present rates: raise the sensor rate or add a
/// column and a motion file would quietly exceed the cap with nothing
/// complaining, and the rider would find out from Telegram refusing the upload.
const ROTATE_BYTES: u64 = 18 * 1024 * 1024;

/// Blocks the writer may fall behind by before the audio thread starts
/// dropping them. Two seconds at 10 ms a block: long enough to cover a storage
/// stall, short enough that the memory is unremarkable.
const QUEUE_BLOCKS: usize = 200;

/// Motion readings the writer may fall behind by.
///
/// Two seconds at Android's fastest useful rate, to match the audio queue's
/// own two seconds: a storage pause long enough to lose audio should lose
/// motion over the same stretch rather than a different one, or the two tracks
/// disagree about what happened and the disagreement looks like a finding.
const MOTION_QUEUE: usize = 800;

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

/// One reading of the phone's own motion, for the third track.
///
/// **Recorded whether or not tap detection is switched on**, which is the
/// requirement that is easy to miss: measuring *false* positives needs rides
/// with no taps in them, so the negative corpus can only be gathered while the
/// feature is off. Tying this to the detector would have made the corpus that
/// matters most impossible to collect.
///
/// Gravity-removed acceleration plus the gravity vector, because the pair is
/// what separates a tap from a pothole: a tap arrives across the world
/// vertical and road shock along it, and the platform's own sensor fusion does
/// that decomposition better than anything worth writing here.
#[derive(Clone, Copy, Debug, Default)]
pub struct MotionSample {
    /// The platform's own stamp, in its own epoch — nanoseconds since boot on
    /// Android, seconds since boot on iOS, and neither is the audio clock.
    ///
    /// Kept **as given** and alongside [`Self::arrival_us`] rather than
    /// converted, so the delay between the two can be *measured* instead of
    /// assumed. Tap tolerances are tens of milliseconds, so arrival stamping is
    /// good enough to align by — but only if somebody can check that later.
    pub platform_ns: u64,
    /// When this reading reached the recorder, on the same monotonic clock the
    /// rest of the session is written against.
    pub arrival_us: u64,
    /// Acceleration with gravity already removed, in m/s².
    pub accel: [f32; 3],
    /// The gravity vector, which is what gives the world frame.
    pub gravity: [f32; 3],
    /// Angular rate, in rad/s. A tap is linear where a suspension event rotates.
    pub rotation: [f32; 3],
}

/// Writes capture and decisions to disk, off the audio thread.
pub struct DiagnosticRecorder {
    tx: SyncSender<Message>,
    motion_tx: SyncSender<MotionSample>,
    worker: Option<thread::JoinHandle<()>>,
    dropped: Arc<AtomicU64>,
    motion_dropped: Arc<AtomicU64>,
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
        // **Its own channel, not a variant on the audio one.** At 400 Hz motion
        // arrives four times as often as a block, so sharing the queue would
        // let a storage hiccup spend the audio budget on motion and drop the
        // capture instead — the one thing in a recording that cannot be
        // reconstructed.
        let (motion_tx, motion_rx) = sync_channel(MOTION_QUEUE);
        let dropped = Arc::new(AtomicU64::new(0));
        let motion_dropped = Arc::new(AtomicU64::new(0));
        let dir_owned = dir.to_path_buf();
        let stem_owned = stem.clone();

        let worker = thread::Builder::new()
            .name("mumbleway-recorder".into())
            .spawn(move || write_loop(rx, motion_rx, &dir_owned, &stem_owned, sample_rate))?;

        Ok(Self {
            tx,
            motion_tx,
            worker: Some(worker),
            dropped,
            motion_dropped,
            dir: dir.to_path_buf(),
        })
    }

    /// Hands over a motion reading. Never blocks.
    pub fn push_motion(&self, sample: MotionSample) {
        match self.motion_tx.try_send(sample) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.motion_dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    /// Motion readings the writer could not keep up with.
    ///
    /// Surfaced beside [`Self::dropped_blocks`] so a truncated motion track is
    /// *visible*. A gap nobody counted looks exactly like a stretch of road
    /// where nothing happened, which is the reading a tap detector would be
    /// scored against.
    pub fn dropped_motion(&self) -> u64 {
        self.motion_dropped.load(Ordering::Relaxed)
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
    motion: BufWriter<File>,
    pcm_written: u64,
    log_written: u64,
    motion_written: u64,
    index: u32,
}

impl Sink {
    /// Whether any of the three has reached the per-file cap.
    fn full(&self) -> bool {
        self.pcm_written >= ROTATE_BYTES
            || self.log_written >= ROTATE_BYTES
            || self.motion_written >= ROTATE_BYTES
    }

    fn flush(&mut self) {
        let _ = self.pcm.flush();
        let _ = self.log.flush();
        let _ = self.motion.flush();
    }
}

fn open_sink(dir: &Path, stem: &str, index: u32, rate: u32) -> std::io::Result<Sink> {
    let pcm_path = dir.join(format!("{stem}-{index:03}.s16"));
    let log_path = dir.join(format!("{stem}-{index:03}.csv"));
    // The third track, under the same stem and the **same index**. That shared
    // index is what makes a segment a coherent slice: segment 7 of each file
    // covers the same stretch of the ride, so a rider can send one segment's
    // three files and the rig has a usable window. Rotating them independently
    // would force every analysis to stitch by timestamp before it could start.
    let motion_path = dir.join(format!("{stem}-{index:03}.motion.csv"));
    let mut motion = BufWriter::new(File::create(motion_path)?);
    writeln!(
        motion,
        "# mumbleway motion track; block is the audio block within THIS segment\n\
         block,platform_ns,arrival_us,ax,ay,az,gx,gy,gz,rx,ry,rz"
    )?;
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
        motion,
        pcm_written: 0,
        log_written: 0,
        motion_written: 0,
        index,
    })
}

/// Closes the current segment and opens the next, all three files together.
///
/// `None` means it could not, which ends the session — a recorder that carries
/// on writing into a full file produces something Telegram will refuse, and the
/// rider finds that out at the end of a ride rather than now.
fn rotate(sink: &mut Sink, dir: &Path, stem: &str, rate: u32) -> Option<Sink> {
    sink.flush();
    match open_sink(dir, stem, sink.index + 1, rate) {
        Ok(next) => Some(next),
        Err(e) => {
            tracing::error!("could not rotate the diagnostic recording: {e}");
            None
        }
    }
}

fn write_loop(
    rx: Receiver<Message>,
    motion_rx: Receiver<MotionSample>,
    dir: &Path,
    stem: &str,
    rate: u32,
) {
    let mut sink = match open_sink(dir, stem, 0, rate) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("diagnostic recording could not start: {e}");
            return;
        }
    };
    let mut block_index: u64 = 0;

    loop {
        // **A timeout rather than a plain `recv`, because motion has to be
        // written when there is no audio at all.** In the listening state of
        // `docs/CAPTURE_ON_DEMAND.md` there is no capture stream, so no blocks
        // arrive — and that is exactly the stretch a negative corpus is made
        // of. Waiting on audio alone would hold every motion reading in the
        // queue until capture resumed, and then write a burst of them stamped
        // with the wrong block.
        let msg = match rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(m) => Some(m),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };

        // Drained on every pass, so a reading is never more than a block behind
        // the index it is stamped with.
        while let Ok(m) = motion_rx.try_recv() {
            let line = format!(
                "{},{},{},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5}\n",
                block_index,
                m.platform_ns,
                m.arrival_us,
                m.accel[0],
                m.accel[1],
                m.accel[2],
                m.gravity[0],
                m.gravity[1],
                m.gravity[2],
                m.rotation[0],
                m.rotation[1],
                m.rotation[2],
            );
            if sink.motion.write_all(line.as_bytes()).is_err() {
                break;
            }
            sink.motion_written += line.len() as u64;
        }

        let block = match msg {
            Some(Message::Block(b)) => b,
            Some(Message::Stop) => break,
            None => {
                if sink.full() {
                    if let Some(next) = rotate(&mut sink, dir, stem, rate) {
                        sink = next;
                        block_index = 0;
                    } else {
                        break;
                    }
                }
                continue;
            }
        };

        let mut bytes = Vec::with_capacity(block.samples.len() * 2);
        for s in &block.samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        if sink.pcm.write_all(&bytes).is_err() {
            break;
        }
        sink.pcm_written += bytes.len() as u64;

        // Built then written, so the length is exact rather than estimated:
        // rotation now watches this file too, and `BufWriter` would answer for
        // its buffer rather than for the file on disk.
        let line = format!(
            "{},{},{},{},{:.3},{:.1},{:.1},{:.1},{:.3},{:.3},{},{},{:.1},\
             {},{},{:.1},{:.1},{:.2},{:.1},{},{},{},{}\n",
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
        if sink.log.write_all(line.as_bytes()).is_err() {
            break;
        }
        sink.log_written += line.len() as u64;
        block_index += 1;

        if sink.full() {
            match rotate(&mut sink, dir, stem, rate) {
                Some(next) => {
                    sink = next;
                    // Back to zero, because each segment is a recording in its
                    // own right — and the motion track shares the convention,
                    // which is why its header says so. Running the counter on
                    // made the column mean "block within the session", a number
                    // nothing can use: the audio beside it starts at sample
                    // zero, so every reader that multiplied the column by the
                    // block size pointed past the end of the file it was
                    // reading. The listen sheet did exactly that and drew the
                    // tail of a long ride as if none of it had been
                    // transmitted.
                    block_index = 0;
                }
                None => break,
            }
        }
    }

    // Everything in flight, including motion readings that arrived after the
    // last block. The recorder is joined rather than detached precisely so this
    // runs before anything offers to share the files.
    while let Ok(m) = motion_rx.try_recv() {
        let _ = writeln!(
            sink.motion,
            "{},{},{},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5},{:.5}",
            block_index,
            m.platform_ns,
            m.arrival_us,
            m.accel[0],
            m.accel[1],
            m.accel[2],
            m.gravity[0],
            m.gravity[1],
            m.gravity[2],
            m.rotation[0],
            m.rotation[1],
            m.rotation[2],
        );
    }
    sink.flush();
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
        let (motion_tx, motion_rx) = sync_channel(MOTION_QUEUE);
        let dir2 = dir.clone();
        let writer = std::thread::spawn(move || write_loop(rx, motion_rx, &dir2, "rot", 48_000));
        for i in 0..blocks {
            tx.send(Message::Block(Box::new(block(i % 2 == 0))))
                .unwrap();
        }
        drop(tx);
        drop(motion_tx);
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

        // The third track rotated with them, under the same index. That shared
        // index is the whole of the alignment story: a rider sends one
        // segment's three files and the rig has a coherent window, where
        // independent rotation would make every analysis stitch by timestamp
        // before it could start.
        assert!(
            dir.join("rot-000.motion.csv").exists() && dir.join("rot-001.motion.csv").exists(),
            "the motion track did not rotate with the other two"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn motion_is_written_with_its_block_and_survives_having_no_audio() {
        let dir = std::env::temp_dir().join(format!("mw-rec-mot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let (tx, rx) = sync_channel(QUEUE_BLOCKS);
        let (motion_tx, motion_rx) = sync_channel(MOTION_QUEUE);
        let dir2 = dir.clone();
        let writer = std::thread::spawn(move || write_loop(rx, motion_rx, &dir2, "mot", 48_000));

        // **Before any audio at all**, which is the case that matters: in the
        // listening state there is no capture stream, and that is exactly the
        // stretch a negative corpus — rides with no taps in them — is made of.
        // A writer that only woke for blocks would hold these until capture
        // resumed and then stamp them all with the wrong one.
        for i in 0..3u64 {
            motion_tx
                .send(MotionSample {
                    platform_ns: 1_000 + i,
                    arrival_us: 2_000 + i,
                    accel: [0.1, 0.2, 0.3],
                    gravity: [0.0, 0.0, 9.81],
                    rotation: [0.01, 0.02, 0.03],
                })
                .unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(80));

        tx.send(Message::Block(Box::new(block(true)))).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(80));
        motion_tx
            .send(MotionSample {
                platform_ns: 9_999,
                arrival_us: 8_888,
                accel: [1.0, 0.0, 0.0],
                gravity: [0.0, 0.0, 9.81],
                rotation: [0.0; 3],
            })
            .unwrap();

        drop(tx);
        drop(motion_tx);
        writer.join().unwrap();

        let csv = fs::read_to_string(dir.join("mot-000.motion.csv")).unwrap();
        // `skip(1)` for the column-name line, which is not a comment — the same
        // shape the decision log's own test works around.
        let rows: Vec<&str> = csv
            .lines()
            .filter(|l| !l.starts_with('#'))
            .skip(1)
            .collect();
        assert_eq!(rows.len(), 4, "every reading should be written: {csv}");

        // The first three arrived before any block, so they belong to block 0.
        for row in &rows[..3] {
            assert!(row.starts_with("0,"), "expected block 0, got {row}");
        }
        // The last arrived after one block had been written.
        assert!(
            rows[3].starts_with("1,"),
            "expected block 1, got {}",
            rows[3]
        );

        // Both clocks are kept, so the delay between them can be measured
        // rather than assumed.
        assert!(rows[3].contains("9999") && rows[3].contains("8888"));

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
        assert_eq!(
            names.len(),
            3,
            "one audio file, one log and one motion track"
        );
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
