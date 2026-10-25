//! Minimal library use: build the attributor once, then attribute requests.
//!
//! `cargo run --example basic`
use traffic_sources::{Attributor, Request};

fn main() {
    // Compiles every rule set; `Attributor` is Send + Sync, so build it once and share it.
    let attributor = Attributor::new();

    let req = Request {
        url: "https://shop.example/p?twclid=abc".into(),
        referer: Some("https://t.co/".into()),
        user_agent: Some(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6_2 like Mac OS X) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Mobile/22G100 Twitter for iPhone/12.31.2"
                .into(),
        ),
        self_hosts: vec!["shop.example".into()],
        ..Default::default()
    };

    let a = attributor.attribute(&req);

    let top = a.top();
    println!(
        "top: {} / {} ({:?}, confidence {:.3}, share {:.3})",
        top.channel.as_str(),
        top.source,
        top.paid,
        top.confidence,
        top.share
    );

    println!("\nall candidates:");
    for c in &a.candidates {
        println!(
            "  {:<16} {:<12} {:.3}",
            c.channel.as_str(),
            c.source,
            c.confidence
        );
    }

    println!("\nwhy (every signal cites the doc it came from):");
    for e in &a.evidence {
        println!(
            "  {:<24} w={:.2}  {}",
            e.signal,
            e.weight,
            e.rule_src.as_deref().unwrap_or("-")
        );
    }

    println!(
        "\nclient: {:?} {:?} surface={:?} app={:?}",
        a.client.os, a.client.form_factor, a.client.surface, a.client.app
    );
    println!("bot: {:?}  new_touch: {}", a.bot, a.new_touch);
}
