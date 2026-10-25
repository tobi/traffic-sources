//! Differential regression test over ~6,000 generated requests and a set of access-log lines.
//!
//! `tests/golden/expected.txt` holds, for every input, the top result plus a 64-bit hash of the
//! *entire* attribution: every candidate, every evidence item, the client and the bot verdict.
//! Any behaviour change anywhere in the engine changes a hash, so it cannot land unnoticed.
//!
//! After an intended change, regenerate and review the diff:
//!
//! ```console
//! UPDATE_GOLDEN=1 cargo test --test golden
//! git diff tests/golden/expected.txt
//! ```
//!
//! A full, human-readable digest of every result is always written to
//! `target/golden-digest.txt`, so two runs can be compared with an ordinary `diff`.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use traffic_sources::*;

const REQUESTS: &str = include_str!("golden/requests.tsv");
const LOG: &str = include_str!("golden/access.log");
const EXPECTED: &str = include_str!("golden/expected.txt");

fn none_if_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn request_from_tsv(line: &str) -> Request {
    let mut f = line.split('\t');
    let url = f.next().unwrap_or("").to_string();
    let referer = none_if_empty(f.next().unwrap_or(""));
    let user_agent = none_if_empty(f.next().unwrap_or(""));
    let self_hosts = f
        .next()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let headers = f
        .filter_map(|h| h.split_once(": "))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<BTreeMap<_, _>>();
    Request {
        url,
        referer,
        user_agent,
        headers,
        self_hosts,
        ..Default::default()
    }
}

fn digest(a: &Attribution) -> String {
    let mut d = String::new();
    for c in &a.candidates {
        let _ = write!(
            d,
            "C {:?}|{}|{}|{:?}|{:?}|{:?}|{:.3}|{:.3}|{:?};",
            c.channel,
            c.source,
            c.medium,
            c.campaign,
            c.paid,
            c.interaction,
            c.confidence,
            c.share,
            c.evidence
        );
    }
    let k = &a.client;
    let _ = write!(
        d,
        "K {:?}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{:.3}|{:?};",
        k.os,
        k.os_version,
        k.os_version_frozen,
        k.form_factor,
        k.device,
        k.browser,
        k.surface,
        k.app,
        k.client_hints_used,
        k.confidence,
        k.rule_src
    );
    if let Some(b) = &a.bot {
        let _ = write!(
            d,
            "B {:?}|{}|{}|{}|{:?};",
            b.class, b.token, b.operator, b.exclude, b.rule_src
        );
    }
    let _ = write!(d, "F {}|{};", a.is_prefetch, a.new_touch);
    for e in &a.evidence {
        let _ = write!(
            d,
            "E {:?}|{}|{:.3}|{:?}|{:?}|{:?}|{:?};",
            e.kind, e.signal, e.weight, e.channel, e.source, e.rule_src, e.verified
        );
    }
    d
}

/// FNV-1a, 64-bit. Stable across platforms and Rust versions, unlike `DefaultHasher`.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn summarise(a: &Attribution, full: &str) -> String {
    let t = a.top();
    format!(
        "{:?} {} {:.3} {:016x}",
        t.channel,
        t.source.replace(' ', "_"),
        t.confidence,
        fnv1a(full)
    )
}

#[test]
fn golden() {
    let e = Attributor::new();
    let mut inputs: Vec<String> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut full = String::new();

    for raw in REQUESTS.lines() {
        let a = e.attribute(&request_from_tsv(raw));
        let d = digest(&a);
        lines.push(summarise(&a, &d));
        let _ = writeln!(full, "{raw}\n  {d}");
        inputs.push(raw.to_string());
    }
    for raw in LOG.lines() {
        let (d, sum) = match parse_combined_log(raw, "shop.example") {
            Some(r) => {
                let a = e.attribute(&r);
                let d = format!(
                    "R {}|{:?}|{:?}|{:?};{}",
                    r.url,
                    r.referer,
                    r.user_agent,
                    r.self_hosts,
                    digest(&a)
                );
                let s = summarise(&a, &d);
                (d, s)
            }
            None => ("unparsed".to_string(), "unparsed".to_string()),
        };
        lines.push(format!("log {sum}"));
        let _ = writeln!(full, "{raw}\n  {d}");
        inputs.push(raw.to_string());
    }

    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    let _ = std::fs::create_dir_all(&target);
    let _ = std::fs::write(target.join("golden-digest.txt"), &full);

    let got = lines.join("\n") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/expected.txt");
        std::fs::write(path, &got).unwrap();
        return;
    }

    let want: Vec<&str> = EXPECTED.lines().collect();
    assert_eq!(
        want.len(),
        lines.len(),
        "corpus size changed; rerun with UPDATE_GOLDEN=1 and review the diff"
    );
    let diffs: Vec<String> = lines
        .iter()
        .zip(&want)
        .enumerate()
        .filter(|(_, (g, w))| g.as_str() != **w)
        .take(25)
        .map(|(i, (g, w))| {
            let input: String = inputs[i].chars().take(220).collect();
            format!("line {}: {input}\n    want {w}\n    got  {g}", i + 1)
        })
        .collect();
    assert!(
        diffs.is_empty(),
        "{} of {} results changed (first {} shown; full digest in target/golden-digest.txt).\n\
         If the change is intended, rerun with UPDATE_GOLDEN=1 and review the diff.\n\n{}",
        lines
            .iter()
            .zip(&want)
            .filter(|(g, w)| g.as_str() != **w)
            .count(),
        lines.len(),
        diffs.len(),
        diffs.join("\n")
    );
}
