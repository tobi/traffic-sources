# TypeScript reference implementation

The original prototype that the Rust crate at the repository root grew out of. It is kept for
reference and for cross-checking behaviour — **the Rust crate is the maintained artifact.**

Differences from the crate:

- Returns a **single** attribution, not a ranked candidate list, and its `confidence` is a flat
  per-signal value rather than a noisy-OR over evidence.
- Narrower taxonomy: `Surface` is `in_app | mobile_web | desktop_web | native_app | bot`, with no
  OS, form-factor or device classification and no nine-class bot split.
- No `src` / `verified` provenance on individual rules.

No dependencies, no build step. Node 22+:

```console
node --experimental-strip-types --test attribution.test.ts
```

## Files

| File | What it is |
|---|---|
| `attribution.ts` | `attribute(req)`, `sessionize(touches)`, `loadExternalReferrers(path)` |
| `attribution.test.ts` | 98 tests |
| `rules.json` | Curated click IDs, in-app UA tokens, `X-Requested-With` packages, referrer hosts, GA4 regexes, bot regex |
| `rules-oss.json` | Matomo host table + search keyword params (CC0-1.0), generated |
| `rules-aibots.json` | AI crawler / user-fetcher split from ai.robots.txt (MIT), generated |
| `build-oss-rules.mjs` | Regenerates `rules-oss.json` and `rules-aibots.json`, and the crate's `rules/oss-referrers.json`, from upstream clones under `docs/oss-survey/src/` |
| `annotate_sources.py` | One-shot generator that produced the crate's `rules/rules.json` from `rules.json` by attaching `src` + `verified` to every rule. Kept for provenance; not idempotent. |

`build-oss-rules.mjs` expects shallow clones of the upstream CC0/MIT lists under
`docs/oss-survey/src/` — see [docs/oss-survey/README.md](../../docs/oss-survey/README.md). Those
directories are gitignored.

Precedence used by this prototype: UTM → click ID → in-app UA token / `X-Requested-With` → weak IDs
(`fbclid`, `igshid`, `srsltid`) → `Referer` (curated, then the OSS list) → `Sec-Fetch` heuristics →
direct.
