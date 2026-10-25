# Changelog

Notable changes. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project uses [semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The crate is renamed **`traffic-attribution` → `traffic-sources`** (lib `traffic_sources`, bin
  `traffic-sources`) and moved to the repository root. The API is unchanged.
- The four share-marker rules (`si`, `s`, `t`, `ref_src`) no longer cite ClearURLs (LGPL-3.0) as
  their derivation. They are bare parameter names observed on first-party share links, and are now
  annotated as such, so no shipped rule traces to a copyleft source.

### Added

- `LICENSE` (MIT), [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md), [`CONTRIBUTING.md`](CONTRIBUTING.md),
  full `Cargo.toml` metadata, and GitHub Actions CI (fmt, clippy, test on stable and MSRV 1.88,
  rustdoc, `cargo package`, a CLI smoke test, and the TypeScript tests on Node 22).
- `examples/basic.rs` and a crate-level doctest, so the README quick start is compiled and run by CI.
- A `licensing` note in `rules/rules.json` → `_provenance` stating the CC0/MIT/Apache/Unlicense-only
  policy for shipped data.

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
