//! S3 — engine spike.
//!
//! Question (ROADMAP.md, M0): is Apple SpeechAnalyzer available and accurate enough, and what
//! is its latency for a ~6 s utterance? The heavy lifting is in `bridge/S3Bridge.swift`,
//! because SpeechAnalyzer is Swift-only. This side parses the bridge's JSON and prints it.
//!
//! Usage:
//!   s3-engine [--locale en-US] [--runs 3] [--download] [AUDIO]
//!
//! AUDIO defaults to fixtures/audio/clean-6s.wav. Anything AVAudioFile reads works: .wav,
//! .m4a, .aiff, .caf. `--download` lets the bridge fetch the on-device model assets for the
//! locale if they are not installed. Runs ≥ 2 give cold vs warm numbers.
//!
//! Paste the whole output back.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::time::Instant;

use serde::Deserialize;

extern "C" {
    fn vox_s3_run(
        path: *const c_char,
        locale: *const c_char,
        runs: i32,
        download: bool,
    ) -> *mut c_char;
    fn vox_s3_free(p: *mut c_char);
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct RunReport {
    index: usize,
    prepare_ms: f64,
    first_result_ms: Option<f64>,
    analyze_ms: f64,
    finalize_ms: f64,
    total_ms: f64,
    volatile_results: usize,
    final_results: usize,
    text: String,
    error: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct Report {
    os: String,
    framework_available: bool,
    transcriber_is_available: Option<bool>,
    supported_locale_count: Option<usize>,
    installed_locales: Option<Vec<String>>,
    requested_locale: String,
    resolved_locale: Option<String>,
    asset_status_before: Option<String>,
    asset_download_ms: Option<f64>,
    asset_status_after: Option<String>,
    reserved_locales: Option<Vec<String>>,
    audio_seconds: Option<f64>,
    audio_format: Option<String>,
    best_analyzer_format: Option<String>,
    runs: Vec<RunReport>,
    #[serde(rename = "peakRssMB")]
    peak_rss_mb: f64,
    error: Option<String>,
}

fn sh(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "?".into())
}

fn rss_now_mb() -> f64 {
    // Resident size of this process right now, from the kernel's task info.
    let out = sh("ps", &["-o", "rss=", "-p", &std::process::id().to_string()]);
    out.trim()
        .parse::<f64>()
        .map(|kb| kb / 1024.0)
        .unwrap_or(f64::NAN)
}

fn opt<T: std::fmt::Display>(v: &Option<T>) -> String {
    v.as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".into())
}

fn main() {
    let mut locale = "en-US".to_string();
    let mut runs: i32 = 3;
    let mut download = false;
    let mut audio: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--locale" => locale = it.next().unwrap_or(locale),
            "--runs" => runs = it.next().and_then(|v| v.parse().ok()).unwrap_or(runs),
            "--download" => download = true,
            "-h" | "--help" => {
                println!("s3-engine [--locale en-US] [--runs 3] [--download] [AUDIO]");
                return;
            }
            other => audio = Some(other.to_string()),
        }
    }
    let audio = audio.unwrap_or_else(|| {
        format!(
            "{}/../../fixtures/audio/clean-6s.wav",
            env!("CARGO_MANIFEST_DIR")
        )
    });

    println!("== S3 engine spike ==");
    println!(
        "macOS {} ({})  chip {}  profile {}",
        sh("sw_vers", &["-productVersion"]),
        sh("sw_vers", &["-buildVersion"]),
        sh("sysctl", &["-n", "machdep.cpu.brand_string"]),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!("audio: {audio}");
    println!("locale: {locale}  runs: {runs}  download: {download}");
    let rss_idle = rss_now_mb();
    println!("RSS before bridge: {rss_idle:.1} MB");
    println!();

    let c_path = CString::new(audio.clone()).unwrap();
    let c_locale = CString::new(locale.clone()).unwrap();
    let t0 = Instant::now();
    // SAFETY: the bridge copies both strings before returning and hands back a strdup'd
    // buffer that we free with the matching function.
    let json = unsafe {
        let p = vox_s3_run(c_path.as_ptr(), c_locale.as_ptr(), runs, download);
        if p.is_null() {
            eprintln!("bridge returned null");
            std::process::exit(2);
        }
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        vox_s3_free(p);
        s
    };
    let wall_ms = t0.elapsed().as_secs_f64() * 1e3;
    let rss_after = rss_now_mb();

    let rep: Report = match serde_json::from_str(&json) {
        Ok(r) => r,
        Err(e) => {
            println!("could not parse bridge JSON ({e}):\n{json}");
            std::process::exit(2);
        }
    };

    println!("bridge OS string:        {}", rep.os);
    println!("framework available:     {}", rep.framework_available);
    println!(
        "SpeechTranscriber.isAvailable: {}",
        opt(&rep.transcriber_is_available)
    );
    println!(
        "supported locales:       {}",
        opt(&rep.supported_locale_count)
    );
    println!(
        "installed locales:       {:?}",
        rep.installed_locales.as_deref().unwrap_or(&[])
    );
    println!(
        "locale:                  requested {} → resolved {}",
        rep.requested_locale,
        opt(&rep.resolved_locale)
    );
    println!(
        "asset status:            before {}  after {}  download {} ms",
        opt(&rep.asset_status_before),
        opt(&rep.asset_status_after),
        rep.asset_download_ms
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "reserved locales:        {:?}",
        rep.reserved_locales.as_deref().unwrap_or(&[])
    );
    println!(
        "audio:                   {} s, {}",
        rep.audio_seconds
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".into()),
        opt(&rep.audio_format)
    );
    println!(
        "analyzer best format:    {}",
        opt(&rep.best_analyzer_format)
    );
    if let Some(e) = &rep.error {
        println!("!! error: {e}");
    }
    println!();

    for r in &rep.runs {
        let label = if r.index == 0 { "cold" } else { "warm" };
        println!("run {} ({label}):", r.index);
        if let Some(e) = &r.error {
            println!("  !! {e}");
            continue;
        }
        let rtf = rep.audio_seconds.map(|s| r.total_ms / 1000.0 / s);
        println!(
            "  prepare {:7.1} ms   first result {:>8}   analyze {:7.1} ms   finalize {:7.1} ms   total {:7.1} ms   RTF {}",
            r.prepare_ms,
            r.first_result_ms.map(|v| format!("{v:.1} ms")).unwrap_or_else(|| "none".into()),
            r.analyze_ms,
            r.finalize_ms,
            r.total_ms,
            rtf.map(|v| format!("{v:.3}")).unwrap_or_else(|| "-".into())
        );
        println!(
            "  results: {} volatile, {} final",
            r.volatile_results, r.final_results
        );
        println!("  text: {:?}", r.text);
    }

    println!();
    println!("== summary ==");
    let ok: Vec<&RunReport> = rep.runs.iter().filter(|r| r.error.is_none()).collect();
    if let Some(first) = ok.first() {
        println!(
            "cold: prepare {:.0} ms + total {:.0} ms",
            first.prepare_ms, first.total_ms
        );
    }
    let warm: Vec<f64> = ok.iter().skip(1).map(|r| r.total_ms).collect();
    if !warm.is_empty() {
        let min = warm.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = warm.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        println!(
            "warm total: min {min:.0} ms  max {max:.0} ms  over {} runs",
            warm.len()
        );
    }
    println!("wall time in bridge: {wall_ms:.0} ms");
    println!(
        "RSS: idle {rss_idle:.1} MB → after {rss_after:.1} MB, peak (ru_maxrss) {:.1} MB",
        rep.peak_rss_mb
    );
    println!(
        "verdict: {}",
        if rep.error.is_none() && !ok.is_empty() {
            "SpeechAnalyzer works on this machine"
        } else {
            "NOT working — see error above"
        }
    );
    if rep.error.is_some() || ok.is_empty() {
        std::process::exit(1);
    }
}
