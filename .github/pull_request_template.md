<!-- Thanks for contributing. Delete anything that does not apply. -->

## What this changes

## Checklist

- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all pass
- [ ] If `reference/ts/` changed: `cd reference/ts && npm test`

If this touches `rules/`:

- [ ] Every new or changed rule has a `src` (documentation URL) and a `verified` date
- [ ] The source is CC0 / MIT / Apache-2.0 / Unlicense / vendor documentation — nothing copied from
      a GPL, AGPL, LGPL or EUPL project (see [CONTRIBUTING.md](../CONTRIBUTING.md))
- [ ] Any user agent added to `tests/sourced.rs` is copied verbatim from a cited source, not
      hand-written
