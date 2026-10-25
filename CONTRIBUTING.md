# Contributing

Thanks for helping. Most contributions here are *rules* — a new click ID, a new in-app browser
token, a bot that showed up in someone's logs — and those have a specific bar, described below.

## Before you push

```console
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

If you touched `reference/ts/`:

```console
cd reference/ts && node --experimental-strip-types --test attribution.test.ts
```

CI runs exactly these, plus a build on the MSRV (currently **1.88**, declared as `rust-version` in
`Cargo.toml`). If you need a newer language feature, raise `rust-version` in the same PR and say so
in the description.

## Adding or changing a rule

Every rule in `rules/rules.json` and `rules/devices.json` carries two mandatory fields. The
`combined_log_parse_and_all_rules_annotated` test fails the build if either is missing, so this is
enforced, not merely requested:

```json
{
  "param": "gclid",
  "source": "google",
  "medium": "cpc",
  "channel": "paid_search",
  "src": "https://support.google.com/google-ads/answer/3095550",
  "verified": "2026-10-24"
}
```

- **`src`** — the URL of the documentation that establishes the fact. Prefer, in order:
  1. the platform's own developer or help documentation (Google Ads help, Meta for Developers,
     OpenAI bot docs, MDN, chromestatus, …);
  2. a permissively licensed open-source list — see the licence rule below;
  3. a reputable secondary source, clearly marked as such in the `src` string, e.g.
     `"https://leadtrackr.io/learn/advertising-click-ids"`.

  If you genuinely cannot cite anything, say so in the string (`"unverified token"`) and set
  `verified` to `null`. An honestly unverified rule is fine; an unsourced one is not.

- **`verified`** — the ISO date (`YYYY-MM-DD`) on which *you* opened `src` and confirmed the rule
  still matches what it says. Not the date the doc was written, and not a date you inherited from a
  neighbouring rule. Use `null` if you did not check.

Re-verifying an existing rule is a welcome contribution on its own: open the `src`, confirm the
behaviour, and bump `verified` to today. The rules currently carrying `null` are listed under
"Still unverified" in [docs/CRITIQUE.md](docs/CRITIQUE.md).

### Licence rule for rule data

**Data copied into `rules/` must be CC0, MIT, Apache-2.0, Unlicense, or vendor documentation.**
Nothing else. This keeps the crate vendorable into closed and permissively licensed projects alike.

That means **do not copy** from Snowplow referer-parser (GPL-3.0), Plausible (AGPL-3.0), ClearURLs
(LGPL-3.0), AdGuard filters (GPL-3.0), Neat-URL (GPL-2.0), Matomo device-detector (LGPL-3.0), or
GoatCounter (EUPL-1.2). You may cite them as prior art, and you may re-derive a *behaviour* from a
published specification that they also implement — but then cite the specification, not them.

If a copyleft list is the only place a table exists, the right answer is to leave it out and let
users load it at runtime with `Attributor::with_external_file` / `--external`. See
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for the full policy.

### Adding a test

New rules should come with a case in `tests/sourced.rs`. The one hard requirement there is in the
file's first line:

> Every UA below is copied verbatim from a cited source (no invented strings).

Do not hand-craft a user agent to make a regex pass. Take a real one from the vendor's
documentation, from uap-core's test corpus, or from a UA sample site, and put the source in a
comment above the constant.

### Regenerating the Matomo table

`rules/oss-referrers.json` is generated, not hand-edited. Shallow-clone the upstream CC0 list into
`docs/oss-survey/src/matomo-org__searchengine-and-social-list/` and run:

```console
node reference/ts/build-oss-rules.mjs
```

`docs/oss-survey/src/` and `docs/oss-survey/data/` are gitignored. Several of the projects the
survey clones are copyleft, so neither the clones nor `data/merged.json` may ever be committed.

## Weights and the confidence model

The weights in `traffic_sources::weights` are hand-set judgements. Changing one moves every score,
so a PR that adjusts a weight should explain the reasoning and show the effect on the existing
tests. Adding a *new* signal kind with its own weight is usually easier to review than re-tuning an
existing one.

## Code style

Default `rustfmt` — the only setting in `rustfmt.toml` is the edition. Clippy runs with
`-D warnings`; if a lint is genuinely wrong for the case at hand, `#[allow]` it locally with a
comment explaining why, rather than loosening it crate-wide.

Keep attribution logic out of the Rust source where a JSON rule would do. The engine should stay
generic; the knowledge belongs in `rules/`.
