//! S4 — footprint spike.
//!
//! Question (ROADMAP.md, M0): idle RSS with SpeechAnalyzer resident, and peak during a
//! dictation. Spawns a command (default: the S3 harness with three runs and a 10 s idle
//! tail), samples its memory every 100 ms with `proc_pid_rusage`, and prints a timeline plus
//! a summary. Because S3 showed the model does not live in our process, it also snapshots
//! every process on the system before and after and reports the ones that grew, so the
//! memory Apple's speech daemon spends on our behalf is visible too.
//!
//! Usage:
//!   s4-footprint [--interval-ms 100] [-- <command> [args...]]
//!
//! Paste the whole output back.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MB: f64 = 1_048_576.0;

#[derive(Clone, Copy)]
struct Sample {
    t_ms: f64,
    rss: f64,
    footprint: f64,
}

fn rusage(pid: i32) -> Option<(f64, f64)> {
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    // SAFETY: RUSAGE_INFO_V4 matches the struct we pass; the kernel fills it or fails.
    let rc = unsafe {
        libc::proc_pid_rusage(
            pid,
            libc::RUSAGE_INFO_V4,
            &mut info as *mut _ as *mut libc::rusage_info_t,
        )
    };
    (rc == 0).then(|| {
        (
            info.ri_resident_size as f64 / MB,
            info.ri_phys_footprint as f64 / MB,
        )
    })
}

/// pid → (rss MB, command) for every process we can see.
fn all_processes() -> HashMap<i32, (f64, String)> {
    let out = Command::new("ps")
        .args(["-axo", "pid=,rss=,comm="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let rss_kb: f64 = it.next()?.parse().ok()?;
            let comm = it.collect::<Vec<_>>().join(" ");
            Some((pid, (rss_kb / 1024.0, comm)))
        })
        .collect()
}

fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut interval_ms: u64 = 100;
    let mut cmd: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--interval-ms" => {
                interval_ms = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(100);
                i += 2;
            }
            "--" => {
                cmd = args[i + 1..].to_vec();
                break;
            }
            "-h" | "--help" => {
                println!("s4-footprint [--interval-ms 100] [-- <command> [args...]]");
                return;
            }
            _ => i += 1,
        }
    }
    if cmd.is_empty() {
        let s3 = format!("{}/../target/release/s3-engine", env!("CARGO_MANIFEST_DIR"));
        cmd = vec![
            s3,
            "--runs".into(),
            "3".into(),
            "--idle-seconds".into(),
            "10".into(),
        ];
    }

    println!("== S4 footprint spike ==");
    println!("command: {}", cmd.join(" "));
    println!("interval: {interval_ms} ms");
    let before = all_processes();
    let sys_before: f64 = before.values().map(|(r, _)| r).sum();
    println!(
        "system: {} processes, {:.0} MB resident total before",
        before.len(),
        sys_before
    );
    println!();

    let t0 = Instant::now();
    let mut child = match Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            println!("cannot spawn {}: {e}", cmd[0]);
            std::process::exit(2);
        }
    };
    let pid = child.id() as i32;
    println!("child pid {pid}");

    // Forward the child's stdout with timestamps so its phases line up with the samples.
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        let mut lines = Vec::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            lines.push((t0.elapsed().as_secs_f64() * 1e3, line));
        }
        lines
    });

    let mut samples: Vec<Sample> = Vec::new();
    // pid → (peak growth over baseline, last seen rss, comm)
    let mut peak_growth: HashMap<i32, (f64, f64, String)> = HashMap::new();
    let mut last_sys_scan = Instant::now() - Duration::from_secs(10);
    loop {
        if let Some((rss, footprint)) = rusage(pid) {
            samples.push(Sample {
                t_ms: t0.elapsed().as_secs_f64() * 1e3,
                rss,
                footprint,
            });
        }
        // System-wide scan every 500 ms: track each process's maximum growth over baseline.
        if last_sys_scan.elapsed() >= Duration::from_millis(500) {
            last_sys_scan = Instant::now();
            for (p, (rss, comm)) in all_processes() {
                if p == pid || p == std::process::id() as i32 {
                    continue;
                }
                let base = before.get(&p).map(|(r, _)| *r).unwrap_or(0.0);
                let growth = rss - base;
                let e = peak_growth.entry(p).or_insert((growth, rss, comm.clone()));
                e.1 = rss;
                if growth > e.0 {
                    e.0 = growth;
                    e.2 = comm;
                }
            }
        }
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
    let status = child.wait().ok();
    let lines = reader.join().unwrap_or_default();

    println!("-- child output (ms since spawn) --");
    for (t, l) in &lines {
        println!("{t:8.0}  {l}");
    }
    println!();
    println!("-- child memory timeline (every ~500 ms) --");
    println!("{:>8}  {:>9}  {:>10}", "t_ms", "rss_MB", "footprint");
    let mut next = 0.0;
    for s in &samples {
        if s.t_ms >= next {
            println!("{:8.0}  {:9.1}  {:10.1}", s.t_ms, s.rss, s.footprint);
            next = s.t_ms + 500.0;
        }
    }
    if let Some(last) = samples.last() {
        println!(
            "{:8.0}  {:9.1}  {:10.1}  (last)",
            last.t_ms, last.rss, last.footprint
        );
    }

    println!();
    println!("== summary ==");
    println!("child exit: {:?}", status.map(|s| s.code()));
    if let Some(peak) = samples
        .iter()
        .max_by(|a, b| a.rss.partial_cmp(&b.rss).unwrap())
    {
        println!(
            "child peak RSS:        {:.1} MB at {:.0} ms",
            peak.rss, peak.t_ms
        );
    }
    if let Some(peak) = samples
        .iter()
        .max_by(|a, b| a.footprint.partial_cmp(&b.footprint).unwrap())
    {
        println!(
            "child peak footprint:  {:.1} MB at {:.0} ms",
            peak.footprint, peak.t_ms
        );
    }
    // Idle: the last 5 s of samples, which is the --idle-seconds tail when using the default.
    let end = samples.last().map(|s| s.t_ms).unwrap_or(0.0);
    let mut idle_rss: Vec<f64> = samples
        .iter()
        .filter(|s| s.t_ms >= end - 5000.0)
        .map(|s| s.rss)
        .collect();
    let mut idle_fp: Vec<f64> = samples
        .iter()
        .filter(|s| s.t_ms >= end - 5000.0)
        .map(|s| s.footprint)
        .collect();
    println!(
        "child idle (last 5 s): RSS {:.1} MB, footprint {:.1} MB, over {} samples",
        median(&mut idle_rss),
        median(&mut idle_fp),
        idle_rss.len()
    );

    let mut movers: Vec<(&i32, &(f64, f64, String))> = peak_growth
        .iter()
        .filter(|(_, (g, _, _))| *g >= 20.0)
        .collect();
    movers.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap());
    println!("system processes that grew ≥ 20 MB during the run:");
    println!("  peak growth | at last scan before child exit | now | process");
    if movers.is_empty() {
        println!("  none");
    }
    let now_all = all_processes();
    for (p, (g, last, comm)) in movers.iter().take(10) {
        let now = now_all.get(p).map(|(r, _)| *r);
        println!(
            "  +{g:7.1} MB | {last:7.1} MB | {} | pid {p} {comm}",
            now.map(|r| format!("{r:7.1} MB"))
                .unwrap_or_else(|| "  exited".into())
        );
    }
    let after = all_processes();
    let sys_after: f64 = after.values().map(|(r, _)| r).sum();
    println!(
        "system resident total: {:.0} MB → {:.0} MB ({:+.0} MB)",
        sys_before,
        sys_after,
        sys_after - sys_before
    );
}
