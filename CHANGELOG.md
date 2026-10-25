# Changelog

Notable changes. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project uses [semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **No dependencies.** `serde`, `serde_json`, `regex` and `url` are replaced by code in the crate:
  a linear-time regex engine, a WHATWG URL parsing subset and a `json` module. Results are
  identical. The golden digest over ~5,900 inputs is byte-for-byte unchanged, and `difftest/`
  checks the engine and parser against `regex` and `url` in CI. See Pass 4 in
  [docs/CRITIQUE.md](docs/CRITIQUE.md) for the two accepted URL edge-case differences.
- **Breaking:** the public types no longer implement `serde::Serialize` / `Deserialize`. Use
  `ToJson::to_json` / `to_json_value` for output, and `Request::from_json` for input.
- **Breaking:** every enum's `as_str()` returns `&'static str` instead of `String`.
- The CLI lists JSON members in struct field order, as the library always did (it went through a
  sorted `serde_json::Value`, so it was alphabetical). Integral numbers are written without a
  fraction (`1`, not `1.0`). The values are the same.
- The rule loader is strict: a field of the wrong type is an error, as are unknown source kinds and
  client-hint values that name no `Os` / `FormFactor`.
- About twice as fast: `attribute` takes 118 ms for 20k requests (was 231 ms), `classify_bot` 8 ms
  (was 57 ms), and `Attributor::new` 4 ms (was 47 ms).
- The crate is renamed **`traffic-attribution` → `traffic-sources`** (lib `traffic_sources`, bin
  `traffic-sources`) and moved to the repository root.
- The four share-marker rules (`si`, `s`, `t`, `ref_src`) no longer cite ClearURLs (LGPL-3.0) as
  their derivation. They are bare parameter names observed on first-party share links, and are now
  annotated as such, so no shipped rule traces to a copyleft source.

### Added

- `traffic_sources::json`: `Value` (parse, lookup, `Display`), the `ToJson` trait and
  `ObjectWriter`.
- `Request::from_json` / `from_json_value` and `RequestError`; enums gain `ALL` and `Display`.
- The CLI's `unparseable` records carry a `reason`.
- A `search_keyword` signal from the shipped per-engine keyword table, which was previously unused.
- `tests/golden.rs`, a golden test that hashes every field of every result over 5,812 generated
  requests and 45 log lines; `tests/cli.rs`, end-to-end tests of the binary; and `difftest/`.
- `LICENSE` (MIT), [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md), [`CONTRIBUTING.md`](CONTRIBUTING.md),
  full `Cargo.toml` metadata, and GitHub Actions CI (fmt, clippy, test on stable and MSRV 1.88,
  rustdoc, `cargo package`, a CLI smoke test, and the TypeScript tests on Node 22).
- `examples/basic.rs` and a crate-level doctest, so the README quick start is compiled and run by CI.
- A `licensing` note in `rules/rules.json` → `_provenance` stating the CC0/MIT/Apache/Unlicense-only
  policy for shipped data.

### Fixed

- `sccid` was double-counted (both `ScCid` and `sccid` fired, 0.93 → 0.995). Click IDs now match
  case-insensitively and the loader rejects case-insensitive duplicates.
- Four bot tokens were listed twice under different casing, so rule order decided the class, and
  for Meta-WebIndexer it decided `exclude`.
- Seventeen bot operators were sentences copied from ai.robots.txt and surfaced as the `source`;
  they are slugs now.
- A prefetched cross-site navigation with no referrer was `direct`; it is dark social like any other.
- `parse_combined_log` treated a `-` user agent as present, so UA-less log lines counted as human.
- `utm_source=threads` lost its last letters to the ad-suffix strip and fell through to referral.
- `parse_combined_log` and the share markers compiled regexes per call (41× slower on logs).
- Candidate ordering is fully deterministic, and NaN can no longer reach a confidence.

### Removed

- `rules.v1.json`, a superseded snapshot of the prototype's rules.

## [0.3.0] — 2026-10-24

The Rust rewrite, with the critic loop recorded in [docs/CRITIQUE.md](docs/CRITIQUE.md).

### Added

- Ranked candidates: `attribute` returns every plausible `(channel, source)` hypothesis with a
  noisy-OR confidence, rather than a single verdict.
- An evidence model — each signal carries a weight, the documentation URL it came from (`src`) and
  the date it was checked (`verified`); candidates index into the evidence list.
- Device taxonomy: 31 device rules covering consoles, TVs, XR headsets, cars, wearables, e-readers,
  HarmonyOS NEXT and KaiOS, plus `Os`, `FormFactor` and `Surface` enums.
- Client hints (`Sec-CH-UA-Platform`, `-Mobile`, `-Model`, `-Platform-Version`, `-Form-Factors`)
  override the user agent; `os_version_frozen` flags UA reduction (`Mac OS X 10_15_7`, `Android 10; K`).
- A nine-class bot taxonomy separating `ai_crawler` from `ai_user_fetch`, plus `Signature-Agent`
  (Web Bot Auth) header detection.
- A conflict discount so that explicit tagging for one channel lowers the competing hypothesis for
  the same source.

### Fixed

- Bot token substring false positives (`Code`, `Dot`, `Muse`, `Trae`) via a word-boundary `RegexSet`
  where the longest documented token wins.
- Regexes are compiled once in a `OnceLock` instead of per request.
- Xbox Edge (`Edge/20`, EdgeHTML) is no longer reported as Chrome.

## [0.2.0] — 2026-10-24

The TypeScript prototype, now at [`reference/ts/`](reference/ts/): GA4 default channel group order,
the AI Assistants channel, the `ai_user_fetch` / `ai_crawler` split, share markers, and the rule
that shipped data stays CC0/MIT/Apache.
