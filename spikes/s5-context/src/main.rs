//! S5 — contextual biasing spike.
//!
//! Question: does Apple SpeechAnalyzer accept contextual hints that bias recognition toward
//! supplied text? The bridge transcribes AUDIO twice, without and with
//! `AnalysisContext.contextualStrings`, and this prints both transcripts and a word diff.
//!
//! Usage: s5-context [--locale en-US] [--context "term one,term two"] [AUDIO]
//! Paste the whole output back, plus what the audio actually says.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

extern "C" {
    fn vox_s5_run(
        path: *const c_char,
        locale: *const c_char,
        context: *const c_char,
    ) -> *mut c_char;
    fn vox_s5_free(p: *mut c_char);
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Run {
    text: String,
    ms: f64,
    prep_ms: Option<f64>,
    #[serde(default)]
    finals: usize,
    #[serde(default)]
    volatiles: usize,
    assets: Option<String>,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    available: bool,
    locale: Option<String>,
    context_terms: Vec<String>,
    order: Vec<String>,
    runs: std::collections::HashMap<String, Run>,
    error: Option<String>,
}

fn hits(text: &str, terms: &[String]) -> usize {
    let lower = text.to_lowercase();
    terms
        .iter()
        .filter(|t| lower.contains(&t.to_lowercase()))
        .count()
}

fn main() {
    let mut locale = "en-US".to_string();
    let mut context = String::new();
    let mut audio: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--locale" => locale = it.next().unwrap_or(locale),
            "--context" => context = it.next().unwrap_or_default(),
            other => audio = Some(other.to_string()),
        }
    }
    let audio = audio.unwrap_or_else(|| {
        format!(
            "{}/../../fixtures/audio/context-6s.wav",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    let terms: Vec<String> = context
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    println!("== S5 context spike ==");
    println!("audio: {audio}");
    println!("context terms: {terms:?}");
    let c_path = CString::new(audio).unwrap();
    let c_locale = CString::new(locale).unwrap();
    let c_ctx = CString::new(terms.join("\n")).unwrap();
    // SAFETY: strings valid for the call; the bridge copies them and returns a strdup'd buffer.
    let json = unsafe {
        let p = vox_s5_run(c_path.as_ptr(), c_locale.as_ptr(), c_ctx.as_ptr());
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        vox_s5_free(p);
        s
    };
    let rep: Report = match serde_json::from_str(&json) {
        Ok(r) => r,
        Err(e) => {
            println!("bad JSON ({e}): {json}");
            std::process::exit(2);
        }
    };
    println!(
        "available: {}  locale: {}",
        rep.available,
        rep.locale.unwrap_or_default()
    );
    if let Some(e) = rep.error {
        println!("!! {e}");
        std::process::exit(1);
    }
    println!();
    let n = rep.context_terms.len();
    for key in &rep.order {
        let Some(run) = rep.runs.get(key) else {
            continue;
        };
        match &run.error {
            Some(e) => println!("{key:<7} ERROR {e}"),
            None => println!(
                "{key:<7} {:5.0} ms{}  terms {}/{n}  results {}f/{}v{}  {:?}",
                run.ms,
                run.prep_ms
                    .map(|p| format!(" (+{p:.0} ms LM prep)"))
                    .unwrap_or_default(),
                hits(&run.text, &rep.context_terms),
                run.finals,
                run.volatiles,
                run.assets
                    .as_ref()
                    .map(|a| format!(" assets {a}"))
                    .unwrap_or_default(),
                run.text
            ),
        }
    }
    println!();
    println!("== summary ==");
    let base = rep
        .runs
        .get("st")
        .map(|r| hits(&r.text, &rep.context_terms))
        .unwrap_or(0);
    for key in &rep.order {
        if let Some(run) = rep.runs.get(key).filter(|r| r.error.is_none()) {
            let h = hits(&run.text, &rep.context_terms);
            println!(
                "{key:<7} recovers {h}/{n} supplied terms ({:+} vs baseline)",
                h as i64 - base as i64
            );
        }
    }
}
