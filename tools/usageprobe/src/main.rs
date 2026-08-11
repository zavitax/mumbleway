//! What this process is costing, asked two ways, on the device that has to answer.
//!
//! The diagnostics panel reports CPU as 0% on both phones and a real number on
//! every desktop. `sysinfo` is the only thing measuring it, and reading its
//! source says iOS is a stub — `refresh_processes_specifics` returns `0` there
//! and populates nothing — but says Android goes through the ordinary Linux
//! backend and *should* work.
//!
//! "Should work" is not a measurement, and Android is a Linux that restricts
//! `/proc` in ways desktops do not. So this asks both, side by side, on the
//! phone:
//!
//! * what `sysinfo` says, exactly as the app asks it;
//! * what `/proc/self/stat` says, read directly.
//!
//! Build it for the phone and run it the way `tools/dfbench` is run — no APK,
//! no Play release. It burns a core for a second between samples so there is
//! something to measure.

use std::time::Instant;

/// Reads user + system jiffies for this process out of `/proc/self/stat`.
///
/// Field 14 and 15, one-based, per `proc(5)`. Parsed from the *last* `)`
/// rather than by splitting on spaces from the start: field 2 is the
/// executable name in brackets and may itself contain spaces and brackets,
/// which is the classic way this parse goes wrong.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn proc_self_ticks() -> Option<(u64, String)> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = after.split_whitespace().collect();
    // `after` starts at field 3, so utime (14) and stime (15) are indices 11
    // and 12 here.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some((utime + stime, format!("utime {utime} stime {stime}")))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn proc_self_ticks() -> Option<(u64, String)> {
    None
}

/// Resident set, from `/proc/self/statm` field 2 in pages.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn proc_self_rss_mb() -> Option<f32> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
    Some((pages * page) as f32 / (1024.0 * 1024.0))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn proc_self_rss_mb() -> Option<f32> {
    None
}

/// The Apple implementation from `app/rust/src/usage.rs`, verbatim, so
/// `cargo check --target aarch64-apple-ios` typechecks it from any host.
#[cfg(target_vendor = "apple")]
fn apple_sample() -> (f64, f32) {
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
        cpu += seconds(threads.system_time.seconds, threads.system_time.microseconds);
    }

    (cpu, memory)
}

/// Per-core system usage, which needs the global `/proc/stat`.
///
/// The question this answers: **can an app show a line per core?** `/proc/stat`
/// is the only source of per-core times on Linux, and it is the file Android
/// denies to `untrusted_app` — the denial that made `sysinfo` report 0% and
/// started all of this. A shell can read it, so a probe run this way proves
/// nothing about the app unless it *fails*.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn per_core_lines() -> Result<Vec<String>, String> {
    let stat = std::fs::read_to_string("/proc/stat").map_err(|e| e.to_string())?;
    Ok(stat
        .lines()
        .filter(|l| l.starts_with("cpu") && !l.starts_with("cpu "))
        .map(|l| l.split_whitespace().take(4).collect::<Vec<_>>().join(" "))
        .collect())
}

/// Per-**thread** CPU, from `/proc/self/task`.
///
/// The alternative to per-core, and the one that lives under `/proc/self` —
/// the tree the app is already reading successfully on Android, since the
/// panel now shows a real number there.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn per_thread() -> Result<Vec<(String, u64)>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc/self/task").map_err(|e| e.to_string())? {
        let dir = entry.map_err(|e| e.to_string())?.path();
        let Ok(stat) = std::fs::read_to_string(dir.join("stat")) else {
            continue;
        };
        let Some(open) = stat.find('(') else { continue };
        let Some(close) = stat.rfind(')') else { continue };
        let name = stat[open + 1..close].to_string();
        let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
        let utime: u64 = fields.get(11).and_then(|v| v.parse().ok()).unwrap_or(0);
        let stime: u64 = fields.get(12).and_then(|v| v.parse().ok()).unwrap_or(0);
        out.push((name, utime + stime));
    }
    Ok(out)
}

fn burn(seconds: f32) {
    let until = Instant::now();
    let mut x = 0u64;
    while until.elapsed().as_secs_f32() < seconds {
        for i in 0..100_000u64 {
            x = x.wrapping_add(i).wrapping_mul(2_654_435_761);
        }
    }
    std::hint::black_box(x);
}

fn main() {
    println!("target: {} {}", std::env::consts::OS, std::env::consts::ARCH);

    let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    println!("clock ticks per second: {ticks_per_sec}");
    println!("readable: /proc/self/stat {}, /proc/self/statm {}",
        std::path::Path::new("/proc/self/stat").exists(),
        std::path::Path::new("/proc/self/statm").exists());
    println!("readable: /proc/stat {}", std::path::Path::new("/proc/stat").exists());

    // Exactly what `app/rust/src/api/mumbleway.rs` does.
    let mut system = sysinfo::System::new();
    let pid = sysinfo::get_current_pid().expect("no pid");
    println!("pid: {pid}");
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);

    let before = proc_self_ticks();
    let started = Instant::now();

    burn(1.0);

    let updated = system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    let elapsed = started.elapsed().as_secs_f64();

    println!("\n--- sysinfo ---");
    println!("processes updated: {updated}");
    match system.process(pid) {
        Some(p) => println!(
            "cpu {:.1}%   memory {:.1} MB",
            p.cpu_usage(),
            p.memory() as f32 / (1024.0 * 1024.0)
        ),
        None => println!("process(pid) -> None   <-- this is what reports 0"),
    }

    println!("\n--- /proc/self ---");
    match (before, proc_self_ticks()) {
        (Some((a, _)), Some((b, detail))) => {
            let cpu = (b - a) as f64 / ticks_per_sec / elapsed * 100.0;
            println!("cpu {cpu:.1}%   ({detail}, over {elapsed:.2}s)");
        }
        _ => println!("unavailable"),
    }
    match proc_self_rss_mb() {
        Some(mb) => println!("memory {mb:.1} MB"),
        None => println!("memory unavailable"),
    }

    #[cfg(target_vendor = "apple")]
    {
        println!("\n--- mach task_info ---");
        let (cpu_seconds, mb) = apple_sample();
        println!("cpu {cpu_seconds:.3} s of CPU consumed   memory {mb:.1} MB");
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        println!("\n--- per core, from the global /proc/stat ---");
        match per_core_lines() {
            Ok(lines) => {
                println!("readable here: {} cores", lines.len());
                for l in lines.iter().take(3) {
                    println!("  {l}");
                }
                println!("  (a shell can read this; an app may not -- see the note above)");
            }
            Err(e) => println!("DENIED: {e}"),
        }

        println!("\n--- per thread, from /proc/self/task ---");
        match per_thread() {
            Ok(mut threads) => {
                threads.sort_by_key(|(_, ticks)| std::cmp::Reverse(*ticks));
                println!("{} threads", threads.len());
                for (name, ticks) in threads.iter().take(6) {
                    println!("  {name:<20} {ticks} ticks");
                }
            }
            Err(e) => println!("DENIED: {e}"),
        }
    }
}
