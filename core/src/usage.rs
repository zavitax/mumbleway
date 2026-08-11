//! What this process costs, asked in a way each platform will actually answer.
//!
//! # Why this is not just `sysinfo`
//!
//! It was, and it reported **0% CPU on both phones** while working on every
//! desktop. Two different causes, found by reading the crate and then by
//! running it on the phone:
//!
//! **iOS**: `sysinfo` compiles its process refresh to a stub there —
//! `refresh_processes_specifics` returns `0` and populates nothing — so
//! `process(pid)` is always `None` and both numbers are zero by construction.
//!
//! **Android**: the Linux backend is real and works. But process CPU usage is
//! computed against the global CPU times, and those come from `/proc/stat`:
//!
//! ```text
//! if self.cpus.is_empty() {
//!     sysinfo_debug!("cannot compute processes CPU usage: no CPU found...");
//!     return;                     // every process's cpu_usage stays 0
//! }
//! ```
//!
//! Android denies `/proc/stat` to an ordinary app while allowing it to the
//! `adb shell` user — which is why `tools/usageprobe` reported 100% from a
//! shell on the same phone the app reported 0% on. Memory does not go through
//! that path and was never affected.
//!
//! # What it does instead
//!
//! Asks only about *this* process, from a source inside the sandbox:
//!
//! | Platform | CPU | Memory |
//! |---|---|---|
//! | Android, Linux | `/proc/self/stat` utime+stime | `/proc/self/statm` |
//! | iOS, macOS | Mach `task_info` on `mach_task_self` | same call's `resident_size` |
//! | Windows | `sysinfo`, which works there | `sysinfo` |
//!
//! Neither Unix path needs to enumerate processes or read anything global, so
//! neither can be refused by a sandbox that still lets the process look at
//! itself. Measured against `sysinfo` on the phone: 99.7% against 100.7%, and
//! the same 3.4 MB.
//!
//! # CPU is a rate, so the first answer is zero
//!
//! Every platform reports CPU as *time consumed*, and a percentage needs two
//! readings. The first call after start has nothing to subtract from and
//! honestly reports zero; the panel polls at 1 Hz, so it is right from the
//! second tick. Reporting a made-up first value would be worse.
//!
//! # A share of the device, not of one core
//!
//! The first version of this reported a share of **one core**, which is what
//! every one of these platform APIs measures, and the Android panel duly read
//! **146%** under load. That is not wrong — this app runs a capture worker, a
//! playback callback, a classifier and a UI, so 1.46 cores is a true statement
//! about a multi-core phone — but a percentage above 100 in a diagnostics
//! panel reads as a broken meter, and it cannot be compared between a
//! four-core phone and an eight-core one.
//!
//! So it is divided by the core count and clamped to 0..=100: **how much of
//! this device we are using**. The same number drives the ladder's CPU rung,
//! which is the other reason it has to mean something absolute — see
//! [`crate::audio::relief`].
//!
//! What that costs is worth stating plainly, because it makes one thing
//! *harder* to see: an app pinning a single core on an eight-core phone now
//! reads 12%, and on a device whose other seven cores are idle that is the
//! honest answer to "how loaded is this phone". The thing that catches a
//! saturated audio thread is the block deadline, which is measured directly
//! and is not this number.

use std::sync::OnceLock;
#[cfg(any(not(target_os = "windows"), test))]
use std::time::Instant;

use parking_lot::Mutex;

/// The previous reading: total CPU seconds this process had used, and when.
///
/// Windows keeps its own state instead — see the implementation there.
#[cfg(not(target_os = "windows"))]
static LAST: OnceLock<Mutex<Option<(f64, Instant)>>> = OnceLock::new();

/// Share of one core as a percentage, and resident memory in mebibytes.
///
/// The Unix platforms report CPU as *time consumed* and this turns it into a
/// rate; Windows is a separate implementation below, because `sysinfo` reports
/// a percentage over its own interval there and running that through the rate
/// as well would be measuring a rate of a rate.
#[cfg(not(target_os = "windows"))]
pub fn process_usage() -> (f32, f32) {
    let (cpu_seconds, memory_mb) = sample();
    let now = Instant::now();

    let cell = LAST.get_or_init(|| Mutex::new(None));
    let mut last = cell.lock();
    let percent = rate(*last, cpu_seconds, now);
    *last = Some((cpu_seconds, now));
    (percent, memory_mb)
}

/// CPU seconds since the previous reading, as a share of one core.
///
/// Pulled out of [`process_usage`] so it can be tested at all: the reading
/// itself is a process-wide singleton, so a test of "the first call reports
/// zero" through that path is only true of whichever test happens to run
/// first — which is how the first version of this test failed.
#[cfg(not(target_os = "windows"))]
fn rate(previous: Option<(f64, Instant)>, seconds: f64, now: Instant) -> f32 {
    let Some((then_seconds, then_at)) = previous else {
        // Nothing to subtract from. Honestly zero rather than invented; the
        // panel polls at 1 Hz, so this is only the first tick after start.
        return 0.0;
    };
    let elapsed = now.duration_since(then_at).as_secs_f64();
    if elapsed <= 0.0 {
        return 0.0;
    }
    // Clamped at zero rather than allowed negative: a counter that appears to
    // go backwards is a platform quirk, not a process that un-ran, and a
    // negative percentage on the panel would be read as a bug in the panel.
    let per_core = (((seconds - then_seconds) / elapsed) * 100.0).max(0.0);
    share_of_device(per_core as f32)
}

/// How many cores the CPU figure is divided by.
///
/// Cached, because `available_parallelism` is a syscall on every platform and
/// this is read once a second for the life of the process. It also cannot
/// change while we are running, so asking twice is asking the same question.
///
/// Falls back to 1 rather than to a guess: on a machine that will not say, a
/// share of one core is the honest reading and it is the conservative one for
/// the ladder, which steps *down* on a high number.
pub fn cores() -> f32 {
    static CORES: OnceLock<f32> = OnceLock::new();
    *CORES.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get() as f32)
            .unwrap_or(1.0)
            .max(1.0)
    })
}

/// Turns a share of one core into a share of the whole device.
///
/// Clamped at 100 rather than allowed to exceed it. The clamp can only bite
/// when the process out-runs its own core count, which means the reading
/// straddled a scheduling artefact — and a panel that says 104% teaches a
/// reader to distrust the number rather than to act on it.
fn share_of_device(per_core: f32) -> f32 {
    (per_core / cores()).clamp(0.0, 100.0)
}

/// Total CPU seconds used by this process, and its resident size in MiB.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn sample() -> (f64, f32) {
    (
        proc_cpu_seconds().unwrap_or(0.0),
        proc_memory_mb().unwrap_or(0.0),
    )
}

/// `utime` and `stime` from `/proc/self/stat`, in seconds.
///
/// Parsed from the **last** `)` rather than by counting spaces from the start.
/// Field 2 is the executable name in brackets, and it may contain both spaces
/// and brackets — which is the classic way this parse goes wrong, and it goes
/// wrong silently by reading two fields that happen to be numbers.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn proc_cpu_seconds() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = after.split_whitespace().collect();
    // `after` begins at field 3, so utime (14) and stime (15) are 11 and 12.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks <= 0 {
        return None;
    }
    Some((utime + stime) as f64 / ticks as f64)
}

/// Resident set from `/proc/self/statm`, field 2, in pages.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn proc_memory_mb() -> Option<f32> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page <= 0 {
        return None;
    }
    Some((pages * page as u64) as f32 / (1024.0 * 1024.0))
}

/// Mach, which answers for the current task inside the iOS sandbox.
///
/// Two calls, and both are needed: `MACH_TASK_BASIC_INFO` carries the resident
/// size and the CPU time of threads that have already **exited**, while
/// `TASK_THREAD_TIMES_INFO` carries the time of the threads still running.
/// Either one alone under-reports, and for this app it under-reports the part
/// that matters — the capture worker is a long-lived thread, so its time is in
/// the second call and never in the first.
#[cfg(target_vendor = "apple")]
fn sample() -> (f64, f32) {
    use mach2::mach_types::task_t;
    use mach2::message::mach_msg_type_number_t;
    use mach2::task::task_info;
    use mach2::task_info::{
        mach_task_basic_info, task_info_t, task_thread_times_info, MACH_TASK_BASIC_INFO,
        MACH_TASK_BASIC_INFO_COUNT, TASK_THREAD_TIMES_INFO, TASK_THREAD_TIMES_INFO_COUNT,
    };
    use mach2::traps::mach_task_self;

    const KERN_SUCCESS: i32 = 0;
    fn seconds(sec: i32, usec: i32) -> f64 {
        sec as f64 + usec as f64 / 1_000_000.0
    }

    let task: task_t = unsafe { mach_task_self() };
    let mut cpu = 0.0;
    let mut memory = 0.0;

    let mut basic = mach_task_basic_info::default();
    let mut count = MACH_TASK_BASIC_INFO_COUNT as mach_msg_type_number_t;
    let ok = unsafe {
        task_info(
            task,
            MACH_TASK_BASIC_INFO,
            &mut basic as *mut _ as task_info_t,
            &mut count,
        )
    };
    if ok == KERN_SUCCESS {
        memory = basic.resident_size as f32 / (1024.0 * 1024.0);
        cpu += seconds(basic.user_time.seconds, basic.user_time.microseconds);
        cpu += seconds(basic.system_time.seconds, basic.system_time.microseconds);
    }

    let mut threads = task_thread_times_info::default();
    let mut count = TASK_THREAD_TIMES_INFO_COUNT as mach_msg_type_number_t;
    let ok = unsafe {
        task_info(
            task,
            TASK_THREAD_TIMES_INFO,
            &mut threads as *mut _ as task_info_t,
            &mut count,
        )
    };
    if ok == KERN_SUCCESS {
        cpu += seconds(threads.user_time.seconds, threads.user_time.microseconds);
        cpu += seconds(
            threads.system_time.seconds,
            threads.system_time.microseconds,
        );
    }

    (cpu, memory)
}

/// Windows, where `sysinfo` works and there is nothing to work around.
///
/// A whole implementation rather than a `sample()` like the others, because
/// `cpu_usage()` is already a percentage measured over `sysinfo`'s own
/// interval. Feeding that through the rate above would take a rate of a rate,
/// and the answer would look plausible and be wrong.
///
/// It keeps its own state for the same reason the Unix path does: the first
/// refresh has nothing to compare against and reports zero.
#[cfg(target_os = "windows")]
pub fn process_usage() -> (f32, f32) {
    static SYSTEM: OnceLock<Mutex<(sysinfo::System, sysinfo::Pid)>> = OnceLock::new();

    let cell = SYSTEM.get_or_init(|| {
        let system = sysinfo::System::new();
        let pid = sysinfo::get_current_pid().unwrap_or(sysinfo::Pid::from(0));
        Mutex::new((system, pid))
    });
    let (system, pid) = &mut *cell.lock();
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[*pid]), true);
    match system.process(*pid) {
        // `cpu_usage` is a share of one core here too, so it needs the same
        // division as the Unix paths -- otherwise the same app reads 146% on a
        // phone and 18% on a desktop for the same load.
        Some(p) => (
            share_of_device(p.cpu_usage()),
            p.memory() as f32 / (1024.0 * 1024.0),
        ),
        None => (0.0, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn the_first_reading_is_zero_and_every_core_busy_reads_as_one_hundred() {
        use std::time::Duration;
        let start = Instant::now();
        let later = start + Duration::from_secs(1);
        let n = cores() as f64;

        // Nothing to compare against.
        assert_eq!(rate(None, 12.0, start), 0.0);

        // Every core busy for a whole second is 100% of the device, whatever
        // the core count is.
        assert!((rate(Some((12.0, start)), 12.0 + n, later) - 100.0).abs() < 0.01);

        // Half the device.
        assert!((rate(Some((12.0, start)), 12.0 + n / 2.0, later) - 50.0).abs() < 0.1);

        // Idle.
        assert_eq!(rate(Some((12.0, start)), 12.0, later), 0.0);

        // A counter that appears to go backwards is a platform quirk, not a
        // process that un-ran. Never negative.
        assert_eq!(rate(Some((12.0, start)), 11.0, later), 0.0);

        // Two readings in the same instant would divide by zero.
        assert_eq!(rate(Some((12.0, start)), 13.0, start), 0.0);
    }

    #[test]
    fn nothing_can_report_more_than_the_whole_device() {
        // The bug this exists for: the panel read 146% on Android, which is a
        // true statement about cores and an unreadable one about a phone.
        assert_eq!(share_of_device(f32::MAX), 100.0);
        assert_eq!(share_of_device(cores() * 100.0), 100.0);
        assert_eq!(share_of_device(-5.0), 0.0);
        assert!(share_of_device(cores() * 50.0) - 50.0 < 0.01);
    }

    #[test]
    fn it_reports_a_plausible_size_and_never_a_negative_share() {
        // Through the real path this time, twice, so the second call has a
        // delta to find. No assertion on the *first* being zero: the reading
        // is a process-wide singleton and another test may have taken it.
        let _ = process_usage();
        let spin = Instant::now();
        let mut x = 0u64;
        while spin.elapsed().as_millis() < 60 {
            x = x.wrapping_add(1);
        }
        std::hint::black_box(x);

        let (cpu, mb) = process_usage();
        assert!(cpu >= 0.0 && cpu.is_finite(), "CPU came back {cpu}");
        // Catches the unit slipping: pages counted as bytes, or bytes reported
        // as mebibytes, both of which produce a number that looks like a
        // number and is wrong by three orders of magnitude.
        assert!(
            (0.5..100_000.0).contains(&mb),
            "resident memory of {mb} MB is not a plausible process size"
        );
    }
}
