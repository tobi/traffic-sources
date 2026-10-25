//! CLI: reads NDJSON `Request` objects or combined-log lines on stdin, writes NDJSON `Attribution`s.
//! Usage: traffic-sources [--host example.com] [--external rules.json] [--top]
use std::io::{BufRead, Write};
use traffic_sources::json::ObjectWriter;
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
    let mut buf = String::new();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let req: Result<Request, String> = if t.starts_with('{') {
            Request::from_json(t)
                .map(|mut r| {
                    if r.self_hosts.is_empty() && !r.url.starts_with("http") {
                        r.self_hosts.push(host.clone());
                    }
                    r
                })
                .map_err(|e| e.to_string())
        } else {
            parse_combined_log(t, &host).ok_or_else(|| "not a combined log line".to_string())
        };
        buf.clear();
        match req {
            Ok(r) => {
                let a = eng.attribute(&r);
                let o = ObjectWriter::new(&mut buf).field("url", &r.url);
                if top {
                    o.field("top", &a.top())
                        .field("client", &a.client)
                        .field("bot", &a.bot)
                        .finish();
                } else {
                    o.field("attribution", &a).finish();
                }
            }
            Err(reason) => ObjectWriter::new(&mut buf)
                .field("error", "unparseable")
                .field("reason", &reason)
                .field("line", t)
                .finish(),
        }
        buf.push('\n');
        if out.write_all(buf.as_bytes()).is_err() {
            // stdout closed (e.g. piped into `head`): stop quietly.
            return;
        }
    }
}
