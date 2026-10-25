//! CLI: reads NDJSON `Request` objects or combined-log lines on stdin, writes NDJSON `Attribution`s.
//! Usage: traffic-sources [--host example.com] [--external rules.json] [--top]
use std::io::{BufRead, Write};
use traffic_sources::{parse_combined_log, Attributor, Request};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("traffic-sources [--host HOST] [--external FILE] [--top]\nstdin: NDJSON {{url,referer,user_agent,headers}} or combined log lines; stdout: NDJSON");
        return;
    }
    let host = arg("--host").unwrap_or_else(|| "localhost".into());
    let top = args.iter().any(|a| a == "--top");
    let mut eng = Attributor::new();
    if let Some(p) = arg("--external") {
        eng = eng.with_external_file(&p).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        });
    }
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout());
    for line in stdin.lock().lines().map_while(Result::ok) {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let req: Option<Request> = if t.starts_with('{') {
            serde_json::from_str::<Request>(t).ok().map(|mut r| {
                if r.self_hosts.is_empty() && !r.url.starts_with("http") {
                    r.self_hosts.push(host.clone());
                }
                r
            })
        } else {
            parse_combined_log(t, &host)
        };
        let v = match req {
            Some(r) => {
                let a = eng.attribute(&r);
                if top {
                    serde_json::json!({"url": r.url, "top": a.top(), "client": a.client, "bot": a.bot})
                } else {
                    serde_json::json!({"url": r.url, "attribution": a})
                }
            }
            None => serde_json::json!({"error": "unparseable", "line": t}),
        };
        let _ = writeln!(out, "{v}");
    }
}
