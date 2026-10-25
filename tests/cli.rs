//! End-to-end tests of the `traffic-sources` binary: stdin in, NDJSON out.
use std::io::Write;
use std::process::{Command, Stdio};
use traffic_sources::json::Value;

const LOG: &str = r#"203.0.113.9 - - [24/Oct/2026:10:00:00 +0000] "GET /p?utm_source=newsletter&utm_medium=email&utm_campaign=oct HTTP/1.1" 200 1024 "-" "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36""#;
const NDJSON: &str = r#"{"url":"https://shop.example/p","referer":"https://www.google.com/","user_agent":"Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"}"#;

fn run(args: &[&str], stdin: &str) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_traffic-sources"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn the CLI");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "exit status {}", out.status);
    let text = String::from_utf8(out.stdout).expect("UTF-8 output");
    assert!(text.is_empty() || text.ends_with('\n'));
    text.lines()
        .map(|l| Value::parse(l).unwrap_or_else(|e| panic!("{e}: {l}")))
        .collect()
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.as_str())
        .collect()
}

#[test]
fn one_line_out_per_non_empty_line_in() {
    let input = format!("{LOG}\n\n   \n{NDJSON}\nnot a log line\n{{\"url\": 3}}\n");
    let out = run(&["--host", "shop.example"], &input);
    assert_eq!(out.len(), 4);

    assert_eq!(keys(&out[0]), ["url", "attribution"]);
    assert_eq!(
        out[0]["url"].as_str(),
        Some("https://shop.example/p?utm_source=newsletter&utm_medium=email&utm_campaign=oct")
    );
    let a = &out[0]["attribution"];
    assert_eq!(
        keys(a),
        [
            "candidates",
            "client",
            "is_prefetch",
            "new_touch",
            "evidence"
        ]
    );
    assert_eq!(a["candidates"][0]["channel"].as_str(), Some("email"));
    assert_eq!(a["client"]["os"].as_str(), Some("macos"));

    assert_eq!(
        out[1]["attribution"]["bot"]["class"].as_str(),
        Some("search_crawler")
    );
    assert_eq!(
        out[1]["attribution"]["bot"]["exclude"].as_bool(),
        Some(true)
    );

    assert_eq!(keys(&out[2]), ["error", "reason", "line"]);
    assert_eq!(out[2]["reason"].as_str(), Some("not a combined log line"));
    assert_eq!(out[2]["line"].as_str(), Some("not a log line"));
    assert_eq!(
        out[3]["reason"].as_str(),
        Some("invalid request: `url` must be a string or null")
    );
}

#[test]
fn top_mode_writes_the_winner_client_and_bot() {
    let out = run(
        &["--host", "shop.example", "--top"],
        &format!("{LOG}\n{NDJSON}\n"),
    );
    assert_eq!(keys(&out[0]), ["url", "top", "client", "bot"]);
    assert_eq!(out[0]["top"]["source"].as_str(), Some("newsletter"));
    assert!(out[0]["bot"].is_null(), "a human visit has a null bot");
    assert_eq!(out[1]["bot"]["token"].as_str(), Some("Googlebot"));
}

#[test]
fn relative_ndjson_urls_take_the_host_flag() {
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
    let line = format!("{{\"url\":\"/x?gclid=1\",\"user_agent\":\"{ua}\"}}\n");
    let out = run(&["--host", "a.example"], &line);
    assert_eq!(out[0]["url"].as_str(), Some("/x?gclid=1"));
    assert_eq!(
        out[0]["attribution"]["candidates"][0]["channel"].as_str(),
        Some("paid_search")
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert!(run(&[], "").is_empty());
}
