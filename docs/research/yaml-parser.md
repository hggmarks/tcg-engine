# Researching a YAML parser for ADR-0004's restricted subset

- Ticket: #16 (research for [05], the loader). This document recommends; [05] decides and records the ADR.
- Date: 2026-11-04. Toolchain: rustc 1.95.0, target `wasm32-unknown-unknown` (the engine's pinned toolchain).
- Versions examined: `serde_yaml` 0.9.34+deprecated, `serde_yaml_ng` 0.10.0, `serde_norway` 0.9.42
  (with `unsafe-libyaml-norway` 0.2.15), `serde_yml` 0.0.13, `saphyr` 0.3.3, `saphyr-parser` 0.3.3,
  `yaml-rust2` 0.13.0 (0.10.4 under `marked-yaml`), `granit-parser` 2.1.0, `serde-saphyr` 2.1.0,
  `marked-yaml` 0.8.0, `libyaml-safer` 0.3.0, `yaml-spanned` 0.0.3, `serde_yaml_bw` 2.5.8, `noyalib` 0.0.58;
  with `serde` 1.0.229 and `serde_path_to_error` 0.1.20.
- Sources: crates.io API (versions, dates, licenses), GitHub API (archived flag, pushes, contributors, issues),
  the RustSec advisory database, each crate's README and its source as downloaded from crates.io, and the
  [serde attribute docs](https://serde.rs/container-attrs.html). Every table cell for the recommended option
  names the spike case that shows it (ids like `a1`, `f2`, see the output at the end).

**Recommendation in one sentence:** load with `serde-saphyr` 2.1.0 (pinned exactly) into plain
`serde::Deserialize` DTOs, using five strict options plus a small event-stream pass that rejects the
YAML core tags the options cannot; `marked-yaml` is the credible runner-up.

## 1. The candidates

**libyaml-based serde crates** (all inherit the same C-to-Rust `unsafe-libyaml` engine):
- `serde_yaml` 0.9.34+deprecated: repository archived on 2024-03-25 (GitHub API `archived: true`). Not
  recommended for new code; kept in the table only as the ancestor.
- `serde_yaml_ng` 0.10.0 (2024-05-26, MIT; repository last pushed 2025-09-14): maintained fork, three releases ever.
- `serde_norway` 0.9.42 (2024-12-21): fork on its own `unsafe-libyaml-norway` 0.2.15. In the spike its output was
  identical to `serde_yaml_ng` in 42 of 42 cases, so the two are treated as one below.
- `serde_yml` 0.0.13: **avoid**. [RUSTSEC-2025-0068](https://github.com/rustsec/advisory-db/blob/main/crates/serde_yml/RUSTSEC-2025-0068.md)
  marks it unsound and unmaintained, the repository is archived, and [advisory-db#2395](https://github.com/rustsec/advisory-db/issues/2395)
  alleges undefined behaviour and AI-generated code. Its newest release (2026-05-27) is a shim forwarding to `noyalib`.
- The shared engine is itself flagged: [RUSTSEC-2024-0449](https://github.com/rustsec/advisory-db/blob/main/crates/unsafe-libyaml/RUSTSEC-2024-0449.md)
  (`unsafe-libyaml` unmaintained, archived 2024-03-25; it names `serde_yaml_ng`; it recommends `saphyr`/`serde-saphyr`),
  and [RUSTSEC-2023-0075](https://github.com/rustsec/advisory-db/blob/main/crates/unsafe-libyaml/RUSTSEC-2023-0075.md)
  (unaligned 64-bit write, undefined behaviour on **wasm32**, fixed in >= 0.2.10).

**Pure-Rust parsers without serde**: `saphyr` / `saphyr-parser` 0.3.3 (2026-10-27, YAML 1.2, MIT OR Apache-2.0, repository
pushed 2026-11-01) and its ancestor `yaml-rust2` 0.13.0 (2026-09-11). Both load a node tree. The spike shows what the tree
keeps (`tree` output): aliases are already substituted and anchors erased, `yaml-rust2` drops every tag, `<<` is not
merged, and scalars are resolved with the YAML 1.2 core schema (`~` becomes null, `1e3` a float, `010` the integer 10).
So **anchors and aliases cannot be rejected by validating the node tree**; they need the event stream.

**Pure-Rust serde layers**:
- `serde-saphyr` 2.1.0 (2026-11-03, MIT OR Apache-2.0): deserializes straight from the parser events (no tree). Builds
  on `granit-parser`, the author's fork of `saphyr-parser` (first release 2026-05-01), re-exported as
  `serde_saphyr::granit_parser`. `#![forbid(unsafe_code)]` on both. Single maintainer (794 commits, then dependabot 45, then another human 12), 39 releases since
  2025-09-27, 2 open issues; a 3.0.0 release candidate is being prepared ([#246](https://github.com/bourumir-wyngs/serde-saphyr/issues/246)),
  so expect breaking releases (that one is about tag identity and enum round-trips, the area this project cares about). Not part of the `saphyr` project (its README says so).
- `marked-yaml` 0.8.0 (2025-05-26, MIT): a deliberately limited loader over `yaml-rust2` 0.10.4. Every scalar is a string,
  mapping keys are scalars, anchors/aliases/tags are load errors ([README](https://github.com/kinnison/marked-data/blob/main/marked-yaml/README.md)).
  One maintainer, last commit on its release day (17 months quiet), 2 open issues, about 4,000 lines including tests.

**Dropped** (one line each): `libyaml-safer` 0.3.0 (events only, no serde); `yaml-spanned` 0.0.3 (0.0.x, 2 stars, builds
on `libyaml-safer`); `serde_yaml_bw` 2.5.8 (libyaml fork by serde-saphyr's author, whose README recommends serde-saphyr);
`noyalib` 0.0.58 (first release 2026-05-10, same author as `serde_yml`, has [RUSTSEC-2026-0333](https://github.com/rustsec/advisory-db/blob/main/crates/noyalib/RUSTSEC-2026-0333.md)).

## 2. Comparison

`ng` = `serde_yaml_ng` and `serde_norway` (identical results; `serde_yaml` itself was not run, it is their parent).
Spike case ids are in brackets.

| | `serde_yaml` (archived) | `ng` | `serde-saphyr` | `marked-yaml` | `saphyr` / `yaml-rust2` |
|---|---|---|---|---|---|
| Anchors / aliases | as `ng` | accepted, aliases resolved [a1, a2] | **rejected** with `Budget { max_anchors: 0, max_aliases: 0 }`, line/col, even an unused anchor [a1, a2] | **rejected** by the loader, line:col [a1, a2] | resolved away in the tree: undetectable after load |
| Tags | as `ng` | accepted and ignored; enum variants are *written as tags* (`!draw`) [i1-i3, enum2] | unknown tags rejected by `reject_unsupported_tags` [i2, enum2]; core tags `!!str`/`!!int` still accepted [i1, i3] | **all rejected**, line:col [i1-i3, enum2] | `yaml-rust2` drops all; `saphyr` keeps non-core only |
| Merge keys `<<` | as `ng` | no merge; `<<` is an ordinary key, only rejected as an unknown field [h1] | **rejected** with `MergeKeyPolicy::Error` (default is to merge) [h1] | no merge; ordinary key, rejected only as unknown field [h1] | no merge |
| Implicit typing | as `ng` | YAML 1.2 core: `yes`/`on` are not bools [b]; `1e3` rejected for ints [e]; but `~`, `null`, empty value become the literal strings `"~"`, `"null"`, `""` for `String` [c/d] | schema-driven. Default accepts YAML 1.1 bools [b]; `strict_booleans` rejects `yes`/`on` [b]; `~`/`null`/empty rejected for `String` [c/d]; `1e3` rejected for ints [e]; `no`/`on`/`1e3` stay literal text [c/d] | none: every scalar is text, typed by Rust `FromStr` [b, c/d, e]; `~` is `"~"`, an empty value is `""` | core schema resolved in the tree (`~` null, `1e3` float) |
| Duplicate keys, struct field | as `ng` | rejected, no location [f1] | rejected by default, line/col [f1] | rejected if `error_on_duplicate_keys(true)` (off by default), both positions [f1] | both parsers error |
| Duplicate keys, map key | as `ng` | **silently keeps the last** [f2-f4] | rejected, also `k` vs `"k"` [f2-f4] | same option as above [f2-f4] | both parsers error |
| Unknown fields | derive | serde `deny_unknown_fields` [g1] | serde `deny_unknown_fields` [g1] | serde `deny_unknown_fields` [g1] | n/a (no serde) |
| Error location | built in | line/col and path [loc1-loc3] | line/col and snippet; path via `serde_path_to_error` [loc1-loc4] | loader errors line:col; typed errors carry a span, but `Display` prints only the path [loc1-loc4] | line/col |
| Enums | as `ng` | `!variant` tags only; map style needs `#[serde(with = "serde_yaml_ng::with::singleton_map_recursive")]` on each field [enum1-3] | single-key map works natively [enum1], also tuple/newtype variants [shape1] | single-key map works [enum1, shape1] | n/a |
| Maintenance | archived 2024-03-25 | `ng`: last release 2024-05-26; `norway`: 2024-12-21; both on an unmaintained C2rust parser | very active, one maintainer, breaking releases every few weeks | one maintainer, quiet since 2025-05-26 | `saphyr` pushed 2026-11-01; `yaml-rust2` 2026-09-11 |
| License | MIT OR Apache-2.0 | `ng` MIT; `norway` MIT OR Apache-2.0 | MIT OR Apache-2.0 | MIT | MIT OR Apache-2.0 |
| Crates in `cargo tree` (itself + deps) | 9 | 9 | 18 (deserialize only, with `serde_path_to_error`); 19 with default features | 22 with `serde-path`; 15 without the serde/derive stack the workspace has anyway | `saphyr` 19, `saphyr-parser` 10, `yaml-rust2` 7 |
| `unsafe` / C-translated | `unsafe-libyaml` (c2rust) | `unsafe-libyaml`/`-norway`: c2rust, 237 lines mentioning `unsafe`; the crates add 59 | own crates forbid unsafe; transitive crates (`simdutf8`, `encoding_rs`, `smallvec`) use it as usual | none in own source; `yaml-rust2` has none either | none in own source |
| `wasm32-unknown-unknown` | builds | builds | builds | builds | builds |

wasm32: I ran `cargo build --target wasm32-unknown-unknown` on a one-dependency probe crate for each of `serde_yaml`,
`serde_yaml_ng`, `serde_norway`, `serde_yml`, `serde-saphyr`, `saphyr`, `saphyr-parser`, `yaml-rust2`, `marked-yaml`,
`libyaml-safer`, `yaml-spanned`, `serde_yaml_bw`, `noyalib`: all 13 build. The spike itself (all of the above except the dropped ones) also builds for wasm32.

Other results that matter for [05]:
- The real `rulesets/small-ruleset/small-ruleset.yaml` passes the event pass and loads under the strict `serde-saphyr` setup and under `marked-yaml` [real file].
- It writes enums as single-key maps (`- draw: { player: each, count: 5 }`) and unit variants as plain strings. That matches `serde-saphyr`
  and `marked-yaml` natively but not `ng` (enum1 vs enum3): the `ng` family would need a format-specific `with` attribute on every enum field, which leaks the YAML crate into the typed model.
- The file also uses scalars whose type depends on data: `init: false` next to `init: { from_field: life }`, and `value: true`. An untagged `Bool | Int | Map` enum reads them under every serde-saphyr/ng backend
  but fails under `marked-yaml` [shape4]. Under the brief's rule ("the target type decides, never the resolver") that enum is resolver-driven typing anyway; see rule (c) in section 4.
- `marked-yaml` also fails `#[serde(tag = "...")]` enums and `#[serde(flatten)]` with integers [shape2, shape3], silently ignores everything after the first `---` document, and does not strip a BOM [robust].
- `serde-saphyr` rejects a second document by default, accepts a BOM, and reports "recursion limit exceeded" for 5000 nested `[` instead of overflowing the stack [robust]. `marked-yaml` also errors on deep nesting.
- `serde` documents `deny_unknown_fields` as unsupported together with `flatten`. In the spike the outer attribute still rejected the unknown field, but the error lost its line/column [g2].

## 3. Where each restriction is enforced (recommended option: `serde-saphyr`)

| Restriction | Layer | Mechanism | Evidence |
|---|---|---|---|
| No anchors, no aliases | parser configuration | `budget! { max_anchors: 0, max_aliases: 0 }`; message reads "budget breached: Anchors", with line/col | a1, a2 |
| No merge keys | parser configuration | `merge_keys: MergeKeyPolicy::Error` | h1, h2 |
| No custom tags (`!custom`, `!variant`) | parser configuration | `reject_unsupported_tags: true` | i2, enum2 |
| No core tags (`!!str`, `!!int`) | **pre-mapping validation** | event pass over `serde_saphyr::granit_parser::Parser` (about 35 lines); the options cannot do it | i1, i3 |
| No implicit typing | schema (the Rust target type) plus parser configuration | `strict_booleans: true`; null forms are already refused for `String`; `no_schema: true` additionally refuses unquoted `true`/`1e3`/`0x1F` for text | b, c/d, e |
| Dynamic scalars (`init`, `value`, catalog values) | DTO design | deserialize as `String`, convert in stage 2 by the declared type | shape4 |
| No duplicate keys (struct fields and map keys) | parser configuration | `duplicate_keys` defaults to `Error` | f1-f4 |
| No unknown fields | **not by the parser**: serde derive on every DTO | `#[serde(deny_unknown_fields)]`, enforced by test and review | g1 |
| One document per file | parser configuration | default of `from_str` | robust |
| Error locations | parser (line/col) plus glue (path) | `with_deserializer_from_str_with_options` + `serde_path_to_error` | loc1-loc4 |
| Order-sensitive structures are lists | **not at all**: DTO types | use `Vec`; `BTreeMap` only for unordered data; ban `HashMap` in DTOs | n/a |

## 4. Recommendation

Use **`serde-saphyr` 2.1.0**, pinned with `=2.1.0`, for rulesets, catalogs and decks.

Why: it rejects anchors, aliases, merge keys, duplicate keys, custom tags and extra documents by configuration alone, each with
a line/column (only core tags need extra code), reads the ruleset's own enum style natively, keeps the typed model on plain `serde::Deserialize` (the format stays interchangeable), has an
unsafe-free parser, and is where the ecosystem is moving (RUSTSEC-2024-0449 names it). Why not the `ng` family: it accepts anchors and
aliases, keeps the last duplicate map key, turns `~` and empty values into text, needs format-specific attributes for
the ruleset's enums, and sits on an unmaintained C-translated parser with a wasm32 UB history.

Why not `marked-yaml` (runner-up): its restrictions hold *by construction* and it is small, which is attractive. But it drops
extra documents silently, does not support the serde shapes `tag`/`flatten`/`untagged`, and has been quiet for 17 months. Revisit
if `serde-saphyr` churn becomes costly: the DTO rules below were chosen so the same DTOs load under both (spike enum1, shape1, loc1-loc4;
only the `Spanned` type differs).

**What [05] must build**
1. One function returning the strict `Options` (the five knobs in the spike's `strict_opts`), with one test per knob copied from the spike cases. `Options` is `#[non_exhaustive]` and defaults can change in a major version: re-run the cases on every upgrade (3.0.0 is coming).
2. The event pass (the spike's `prevalidate`) if core tags are to be banned; it also gives every restriction one uniform message. Skip it only if `!!str` and friends are accepted as harmless explicit typing.
3. Path glue (about 8 lines): `with_deserializer_from_str_with_options` + `serde_path_to_error`, producing `file:line:col path: message`.
4. DTO rules, enforced by test or lint:
   (a) `#[serde(deny_unknown_fields)]` on every DTO, no `#[serde(flatten)]`;
   (b) enums externally tagged: variants as single-key maps, unit variants as plain strings, never `!tag` syntax;
   (c) any scalar whose type depends on data (ruleset `init`/`value`, catalog field values typed by the ruleset's card-type schema) is a `String` in the DTO and converted in stage 2 by the declared type; no `#[serde(untagged)]` over scalars;
   (d) no `HashMap` in DTOs.
5. Stage-2 (semantic) errors with locations: `serde_saphyr::Spanned<T>` on DTO leaf fields (works inside enum variants, not inside `flatten`, `untagged` or internally tagged enums). Keep it out of the engine's typed model; convert DTO to model.

**Residual decisions for [05]** (not restrictions the options can express): `True`/`TRUE` are accepted as booleans (`strict_booleans` allows the YAML 1.2 core spellings) [b True]; integers accept `0x10`, `1_000` and `+1` but reject `010` and `1e3` [e], so parse integers from `String` in stage 2 if decimal-only is wanted; a text value that is literally `null` or `~` must be quoted, which is a good outcome; `no_schema: true` is optional.

ADR-0004 is achievable as written. No change to it is needed.

## 5. The spike

Standalone, never in the repository: `/tmp/yaml-spike/spike/` (package `yaml-spike`, binaries `yaml-spike` and `tree`).
Run: `cd /tmp/yaml-spike/spike && cargo run --bin yaml-spike -- <path to small-ruleset.yaml>` and `cargo run --bin tree`.
Each case loads one snippet through `saphyr default` (library defaults), `saphyr options-only` (the five knobs), `saphyr RECOMMENDED`
(knobs + event pass + path), `serde_yaml_ng`, `serde_norway`, `yaml_ng + pre-pass` (the same event pass in front of `serde_yaml_ng`) and `marked-yaml`.
Rows for `REC+no_schema` and `serde_norway` appear only when they differ from `RECOMMENDED` and `serde_yaml_ng`. Lines longer than 118 characters are cut.

### 5.1 Project files

`rust-toolchain.toml`
```toml
[toolchain]
channel = "1.95.0"
targets = ["wasm32-unknown-unknown"]
```

`Cargo.toml`
```toml
[package]
name = "yaml-spike"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
serde = { version = "1", features = ["derive"] }
serde-saphyr = "2.1.0"
serde_yaml_ng = "0.10.0"
serde_norway = "0.9.42"
saphyr = "0.3.3"
yaml-rust2 = "0.13.0"
serde_path_to_error = "0.1"
marked-yaml = { version = "0.8.0", features = ["serde-path"] }
```

### 5.2 Code: `src/main.rs`

```rust
//! Standalone spike for ticket #16 (never part of the tcg-engine workspace).
//! Loads one YAML snippet into a typed target through several backends and prints each outcome.
//! "Schema-driven typing": the Rust target type decides how a scalar is read.
#![allow(dead_code)]
use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_saphyr::granit_parser::{Event, Parser, ScalarStyle, Span};

// ---- typed targets (stand-ins for ruleset DTOs)
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct Flag { flag: bool }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct Text { text: String }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct MaybeText { text: Option<String> }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct Num { n: i64 }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct Real { x: f64 }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct Known { a: i64 }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct Pair { a: i64, b: i64 }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct ZoneDef { id: String, ordered: bool }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct Doc { zones: Vec<ZoneDef> }
#[derive(Debug, Deserialize)] struct Inner { b: Option<i64> }
#[derive(Debug, Deserialize)] #[serde(deny_unknown_fields)] struct Flat { a: i64, #[serde(flatten)] inner: Inner }
/// Externally tagged enum, as `small-ruleset.yaml` writes it: `- draw: { count: 2 }`, `- pass`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Step { Draw { count: u32 }, Pass }
#[derive(Debug, Deserialize)] struct Steps { list: Vec<Step> }
#[derive(Debug, Deserialize)]
struct StepsAdapter { #[serde(with = "serde_yaml_ng::with::singleton_map_recursive")] list: Vec<Step> }
#[derive(Debug, Deserialize)] #[serde(rename_all = "snake_case")] enum Amount { Fixed(u32), Range(u32, u32) }
#[derive(Debug, Deserialize)] struct Amounts { list: Vec<Amount> }
#[derive(Debug, Deserialize)] #[serde(tag = "op", rename_all = "snake_case")] enum Op { Draw { count: u32 } }
/// `init: false` or `init: { from_field: life }`, both occur in small-ruleset.yaml.
#[derive(Debug, Deserialize)] #[serde(untagged)] enum Init { Bool(bool), Int(i64), FromField { from_field: String } }
#[derive(Debug, Deserialize)] struct InitDoc { init: Init }
#[derive(Debug, Deserialize)] struct IntInner { b: i64 }
#[derive(Debug, Deserialize)] struct FlatInt { a: i64, #[serde(flatten)] inner: IntInner }
#[derive(Debug, Deserialize)]
struct LocatedM { id: marked_yaml::Spanned<String>, steps: Vec<LStepM> }
#[derive(Debug, Deserialize)] #[serde(rename_all = "snake_case")]
enum LStepM { Move { to: marked_yaml::Spanned<String> } }
#[derive(Debug, Deserialize)]
struct Located { id: serde_saphyr::Spanned<String>, steps: Vec<LStep> }
#[derive(Debug, Deserialize)] #[serde(rename_all = "snake_case")]
enum LStep { Move { to: serde_saphyr::Spanned<String> } }

// ---- backends
type Load<T> = fn(&str) -> Result<T, String>;

/// Event-stream validation pass: rejects anchors, aliases, tags, merge keys, non-scalar keys and
/// duplicate keys, with line/column, before any mapping. Backend-independent: it reads the event
/// stream of granit-parser, which serde-saphyr re-exports as `serde_saphyr::granit_parser`.
fn prevalidate(src: &str) -> Result<(), String> {
    enum Frame { Seq, Map { expect_key: bool, keys: HashSet<String> } }
    fn flip(stack: &mut [Frame]) {
        if let Some(Frame::Map { expect_key, .. }) = stack.last_mut() { *expect_key = !*expect_key; }
    }
    let at = |s: &Span| format!("line {} column {}", s.start.line(), s.start.col() + 1);
    let mut stack: Vec<Frame> = Vec::new();
    for ev in Parser::new_from_str(src) {
        let (ev, span) = ev.map_err(|e| format!("syntax error: {e}"))?;
        let key_pos = matches!(stack.last(), Some(Frame::Map { expect_key: true, .. }));
        let is_map = matches!(ev, Event::MappingStart(..));
        match ev {
            Event::Alias(_) => return Err(format!("alias not allowed at {}", at(&span))),
            Event::Scalar(v, style, anchor, tag) => {
                if anchor != 0 { return Err(format!("anchor not allowed at {}", at(&span))); }
                if tag.is_some() { return Err(format!("tag not allowed at {}", at(&span))); }
                if key_pos {
                    if style == ScalarStyle::Plain && v == "<<" { return Err(format!("merge key not allowed at {}", at(&span))); }
                    if let Some(Frame::Map { keys, .. }) = stack.last_mut() {
                        if !keys.insert(v.to_string()) { return Err(format!("duplicate key `{v}` at {}", at(&span))); }
                    }
                }
                flip(&mut stack);
            }
            Event::SequenceStart(_, anchor, tag) | Event::MappingStart(_, anchor, tag) => {
                if anchor != 0 { return Err(format!("anchor not allowed at {}", at(&span))); }
                if tag.is_some() { return Err(format!("tag not allowed at {}", at(&span))); }
                if key_pos { return Err(format!("non-scalar key at {}", at(&span))); }
                stack.push(if is_map { Frame::Map { expect_key: true, keys: HashSet::new() } } else { Frame::Seq });
            }
            Event::SequenceEnd | Event::MappingEnd => { stack.pop(); flip(&mut stack); }
            _ => {}
        }
    }
    Ok(())
}

fn strict_opts() -> serde_saphyr::Options {
    serde_saphyr::options! {
        strict_booleans: true,                                         // only true/false (any case of the 1.2 core set)
        merge_keys: serde_saphyr::options::MergeKeyPolicy::Error,      // `<<` rejected
        reject_unsupported_tags: true,                                 // `!custom`, `!variant` rejected
        reject_tagged_field_names: true,
        budget: serde_saphyr::budget! { max_anchors: 0, max_aliases: 0 }, // any `&a` or `*a` rejected
        with_snippet: false,                                           // plain `... at line L, column C`
    }                                                                  // duplicate_keys defaults to Error
}
fn with_path<T: DeserializeOwned>(s: &str, opts: serde_saphyr::Options) -> Result<T, String> {
    let mut path = String::new();
    let r = serde_saphyr::with_deserializer_from_str_with_options(s, opts, |de| {
        serde_path_to_error::deserialize::<_, T>(de).map_err(|e| { path = e.path().to_string(); e.into_inner() })
    });
    r.map_err(|e| format!("{e} [path: {path}]"))
}
fn sp_default<T: DeserializeOwned>(s: &str) -> Result<T, String> { serde_saphyr::from_str(s).map_err(|e| e.to_string()) }
fn sp_options<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_saphyr::from_str_with_options(s, strict_opts()).map_err(|e| e.to_string())
}
/// RECOMMENDED: event pre-pass + strict options + error path via serde_path_to_error.
fn sp_rec<T: DeserializeOwned>(s: &str) -> Result<T, String> { prevalidate(s)?; with_path(s, strict_opts()) }
/// Recommended plus `no_schema`: unquoted scalars that look like bool/number/null are refused for String.
fn sp_rec_noschema<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    prevalidate(s)?;
    let mut o = strict_opts();
    o.no_schema = true;
    with_path(s, o)
}
/// marked-yaml: every scalar is a string; anchors, aliases and tags are parse errors.
/// Typed errors carry a Span, but `Display` prints only the path, so the loader formats the position itself.
fn marked<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    use marked_yaml::Error as E;
    let opts = marked_yaml::LoaderOptions::default().error_on_duplicate_keys(true);
    let node = marked_yaml::parse_yaml_with_options(0, s, opts).map_err(|e| e.to_string())?;
    marked_yaml::from_node(&node).map_err(|e| {
        let (E::NotBoolean(sp) | E::IntegerParseFailure(_, sp) | E::FloatParseFailure(_, sp)
            | E::UnknownFieldError(_, _, sp) | E::Other(_, sp)) = &*e;
        format!("{e} @ line:col {}", sp.start().map_or("?".to_string(), |m| m.to_string()))
    })
}
fn ng<T: DeserializeOwned>(s: &str) -> Result<T, String> { serde_yaml_ng::from_str(s).map_err(|e| e.to_string()) }
fn norway<T: DeserializeOwned>(s: &str) -> Result<T, String> { serde_norway::from_str(s).map_err(|e| e.to_string()) }
fn ng_pre<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    prevalidate(s)?;
    serde_yaml_ng::from_str(s).map_err(|e| e.to_string())
}

fn show<T: Debug>(r: Result<T, String>) -> String {
    let s = match r { Ok(v) => format!("OK  {v:?}"), Err(e) => format!("ERR {}", e.replace('\n', " | ")) };
    if s.chars().count() > 118 { format!("{}...", s.chars().take(118).collect::<String>()) } else { s }
}
static CASES: AtomicUsize = AtomicUsize::new(0);
static DIFFERS: AtomicUsize = AtomicUsize::new(0);
/// The `serde_norway` and `REC+no_schema` rows are printed only when they differ from their sibling.
fn run<T: DeserializeOwned + Debug>(id: &str, yaml: &str) {
    println!("### {id}   {}", yaml.replace('\n', "\\n").replace('\t', "\\t"));
    let rec = show(sp_rec::<T>(yaml));
    let nosch = show(sp_rec_noschema::<T>(yaml));
    let ng_ = show(ng::<T>(yaml));
    let mk = show(marked::<T>(yaml));
    let nw = show(norway::<T>(yaml));
    println!("  saphyr default        {}", show(sp_default::<T>(yaml)));
    println!("  saphyr options-only   {}", show(sp_options::<T>(yaml)));
    println!("  saphyr RECOMMENDED    {rec}");
    if nosch != rec { println!("  saphyr REC+no_schema  {nosch}"); }
    println!("  serde_yaml_ng         {ng_}");
    CASES.fetch_add(1, Ordering::Relaxed);
    if nw != ng_ { println!("  serde_norway          {nw}"); DIFFERS.fetch_add(1, Ordering::Relaxed); }
    println!("  yaml_ng + pre-pass    {}", show(ng_pre::<T>(yaml)));
    println!("  marked-yaml           {mk}");
}

fn main() {
    // a. anchors and aliases
    run::<Pair>("a1", "a: &x 1\nb: *x\n");
    run::<Known>("a2 (anchor never aliased)", "a: &x 1\n");
    // b. bool field
    for v in ["yes", "on", "True"] { run::<Flag>(&format!("b {v}"), &format!("flag: {v}\n")); }
    // c/d. String field
    for v in ["no", "on", "1e3", "true", "~", "\"no\""] { run::<Text>(&format!("c/d {v}"), &format!("text: {v}\n")); }
    run::<Text>("c/d (empty value)", "text:\n");
    run::<MaybeText>("d' Option<String> = ~", "text: ~\n");
    // e. integer field
    for v in ["1e3", "0x10", "1_000", "010"] { run::<Num>(&format!("e {v}"), &format!("n: {v}\n")); }
    run::<Real>("e' f64 = 1e3", "x: 1e3\n");
    // f. duplicate keys
    run::<Known>("f1 struct field", "a: 1\na: 2\n");
    run::<BTreeMap<String, i64>>("f2 map key", "k: 1\nk: 2\n");
    run::<BTreeMap<String, i64>>("f3 map key, flow style", "{k: 1, k: 2}\n");
    run::<BTreeMap<String, i64>>("f4 k vs \"k\"", "k: 1\n\"k\": 2\n");
    // g. unknown fields
    run::<Known>("g1", "a: 1\nzzz: 2\n");
    run::<Flat>("g2 deny_unknown_fields + flatten", "a: 1\nzzz: 2\n");
    // h. merge keys
    run::<Pair>("h1 inline", "<<: {a: 1}\nb: 2\n");
    run::<BTreeMap<String, BTreeMap<String, i64>>>("h2 with anchor", "x: &d {a: 1}\ny:\n  <<: *d\n  b: 2\n");
    // i. tags
    run::<Text>("i1 !!str", "text: !!str 1\n");
    run::<Text>("i2 !custom", "text: !custom x\n");
    run::<Num>("i3 !!int", "n: !!int 5\n");
    // enums
    run::<Steps>("enum1 single-key map", "list:\n  - draw: { count: 2 }\n  - pass\n");
    run::<Steps>("enum2 YAML tags", "list:\n  - !draw { count: 2 }\n  - !pass\n");
    run::<StepsAdapter>("enum3 map style + singleton_map_recursive", "list:\n  - draw: { count: 2 }\n  - pass\n");
    // error locations and paths
    run::<Doc>("loc1 wrong type", "zones:\n  - { id: deck, ordered: true }\n  - { id: hand, ordered: maybe }\n");
    run::<Doc>("loc2 unknown field", "zones:\n  - { id: deck, ordered: true }\n  - { id: hand, ordered: false, extra: 1 }\n");
    run::<Doc>("loc3 syntax error", "zones:\n  - { id: deck, ordered: true\n");
    run::<Steps>("loc4 inside an enum variant", "list:\n  - pass\n  - draw: { count: two }\n");
    // DTO shapes that depend on how the deserializer treats scalars
    run::<Amounts>("shape1 newtype/tuple variants", "list:\n  - fixed: 3\n  - range: [1, 2]\n");
    run::<Op>("shape2 internally tagged enum", "op: draw\ncount: 2\n");
    run::<InitDoc>("shape4a untagged Bool|Int|Map, bool", "init: false\n");
    run::<InitDoc>("shape4b untagged Bool|Int|Map, map", "init: { from_field: life }\n");
    run::<FlatInt>("shape3 flatten with integer fields", "a: 1\nb: 2\n");
    // spans for stage-2 (semantic) errors
    run::<Located>("span Spanned<T>", "id: deck\nsteps:\n  - move: { to: hand }\n");
    println!("### serde_norway output differed from serde_yaml_ng in {} of {} cases", DIFFERS.load(Ordering::Relaxed), CASES.load(Ordering::Relaxed));
    // malformed or unusual documents: must be an error or a clean load, never a panic
    let nested = "[".repeat(5000);
    for (id, y) in [("undefined alias", "a: *x\n"), ("two documents", "a: 1\n---\na: 2\n"), ("empty file", ""), ("BOM + CRLF", "\u{feff}a: 1\r\n"), ("5000 nested [", nested.as_str())] {
        let r = std::panic::catch_unwind(|| (show(marked::<Known>(y)), show(sp_rec::<Known>(y))));
        match r {
            Ok((m, s)) => println!("### robust {id}\n  marked-yaml           {m}\n  saphyr RECOMMENDED    {s}"),
            Err(_) => println!("### robust {id}: PANIC"),
        }
    }
    println!("### span (marked_yaml::Spanned)");
    println!("  marked-yaml           {}", show(marked::<LocatedM>("id: deck\nsteps:\n  - move: { to: hand }\n")));
    // the repository's real ruleset file, passed as CLI argument
    for path in std::env::args().skip(1) {
        let src = std::fs::read_to_string(&path).expect("readable file");
        println!("### real file {path}");
        println!("  pre-pass            {:?}", prevalidate(&src));
        println!("  saphyr RECOMMENDED (data ignored) {:?}", sp_rec::<serde::de::IgnoredAny>(&src).map(|_| "OK"));
        println!("  marked-yaml        (data ignored) {:?}", marked::<serde::de::IgnoredAny>(&src).map(|_| "OK"));
    }
}
```

### 5.3 Code: `src/bin/tree.rs` (node-tree parsers)

```rust
//! Probe of the two node-tree parsers (no serde): what the loaded tree looks like for each
//! restricted-subset violation. Tells us what a tree-validation pass would see.
use saphyr::{LoadableYamlNode, Yaml};

fn show(title: &str, src: &str) {
    println!("\n### {title}\nyaml: {}", src.replace('\n', "\\n"));
    match Yaml::load_from_str(src) {
        Ok(docs) => println!("  saphyr 0.3.3     OK  {docs:?}"),
        Err(e) => println!("  saphyr 0.3.3     ERR {e}"),
    }
    match yaml_rust2::YamlLoader::load_from_str(src) {
        Ok(docs) => println!("  yaml-rust2 0.13  OK  {docs:?}"),
        Err(e) => println!("  yaml-rust2 0.13  ERR {e}"),
    }
}

fn main() {
    show("anchor + alias", "a: &x 1\nb: *x\n");
    show("duplicate key", "k: 1\nk: 2\n");
    show("merge key", "<<: {a: 1}\nb: 2\n");
    show("tags", "a: !!str 1\nb: !custom x\n");
    show("implicit typing: yes no on ~ 1e3 0x1F 010 null", "a: yes\nb: no\nc: on\nd: ~\ne: 1e3\nf: 0x1F\ng: 010\nh: null\ni: True\n");
    show("quoted forms stay strings", "a: \"yes\"\nb: '~'\nc: '1e3'\n");
}
```

### 5.4 Output of `yaml-spike` (verbatim)

```text
### a1   a: &x 1\nb: *x\n
  saphyr default        OK  Pair { a: 1, b: 1 }
  saphyr options-only   ERR budget breached: Anchors { anchors: 1 } at line 1, column 7
  saphyr RECOMMENDED    ERR anchor not allowed at line 1 column 7
  serde_yaml_ng         OK  Pair { a: 1, b: 1 }
  yaml_ng + pre-pass    ERR anchor not allowed at line 1 column 7
  marked-yaml           ERR 1:7: Unexpected definition of anchor
### a2 (anchor never aliased)   a: &x 1\n
  saphyr default        OK  Known { a: 1 }
  saphyr options-only   ERR budget breached: Anchors { anchors: 1 } at line 1, column 7
  saphyr RECOMMENDED    ERR anchor not allowed at line 1 column 7
  serde_yaml_ng         OK  Known { a: 1 }
  yaml_ng + pre-pass    ERR anchor not allowed at line 1 column 7
  marked-yaml           ERR 1:7: Unexpected definition of anchor
### b yes   flag: yes\n
  saphyr default        OK  Flag { flag: true }
  saphyr options-only   ERR invalid boolean (strict mode expects true/false) at line 1, column 7
  saphyr RECOMMENDED    ERR invalid boolean (strict mode expects true/false) at line 1, column 7 [path: flag]
  serde_yaml_ng         ERR flag: invalid type: string "yes", expected a boolean at line 1 column 7
  yaml_ng + pre-pass    ERR flag: invalid type: string "yes", expected a boolean at line 1 column 7
  marked-yaml           ERR flag: Value was not a boolean @ line:col 1:7
### b on   flag: on\n
  saphyr default        OK  Flag { flag: true }
  saphyr options-only   ERR invalid boolean (strict mode expects true/false) at line 1, column 7
  saphyr RECOMMENDED    ERR invalid boolean (strict mode expects true/false) at line 1, column 7 [path: flag]
  serde_yaml_ng         ERR flag: invalid type: string "on", expected a boolean at line 1 column 7
  yaml_ng + pre-pass    ERR flag: invalid type: string "on", expected a boolean at line 1 column 7
  marked-yaml           ERR flag: Value was not a boolean @ line:col 1:7
### b True   flag: True\n
  saphyr default        OK  Flag { flag: true }
  saphyr options-only   OK  Flag { flag: true }
  saphyr RECOMMENDED    OK  Flag { flag: true }
  serde_yaml_ng         OK  Flag { flag: true }
  yaml_ng + pre-pass    OK  Flag { flag: true }
  marked-yaml           OK  Flag { flag: true }
### c/d no   text: no\n
  saphyr default        OK  Text { text: "no" }
  saphyr options-only   OK  Text { text: "no" }
  saphyr RECOMMENDED    OK  Text { text: "no" }
  serde_yaml_ng         OK  Text { text: "no" }
  yaml_ng + pre-pass    OK  Text { text: "no" }
  marked-yaml           OK  Text { text: "no" }
### c/d on   text: on\n
  saphyr default        OK  Text { text: "on" }
  saphyr options-only   OK  Text { text: "on" }
  saphyr RECOMMENDED    OK  Text { text: "on" }
  serde_yaml_ng         OK  Text { text: "on" }
  yaml_ng + pre-pass    OK  Text { text: "on" }
  marked-yaml           OK  Text { text: "on" }
### c/d 1e3   text: 1e3\n
  saphyr default        OK  Text { text: "1e3" }
  saphyr options-only   OK  Text { text: "1e3" }
  saphyr RECOMMENDED    OK  Text { text: "1e3" }
  saphyr REC+no_schema  ERR The string value [1e3] must be quoted at line 1, column 7 [path: text]
  serde_yaml_ng         OK  Text { text: "1e3" }
  yaml_ng + pre-pass    OK  Text { text: "1e3" }
  marked-yaml           OK  Text { text: "1e3" }
### c/d true   text: true\n
  saphyr default        OK  Text { text: "true" }
  saphyr options-only   OK  Text { text: "true" }
  saphyr RECOMMENDED    OK  Text { text: "true" }
  saphyr REC+no_schema  ERR The string value [true] must be quoted at line 1, column 7 [path: text]
  serde_yaml_ng         OK  Text { text: "true" }
  yaml_ng + pre-pass    OK  Text { text: "true" }
  marked-yaml           OK  Text { text: "true" }
### c/d ~   text: ~\n
  saphyr default        ERR error: line 1 column 7: cannot deserialize null into string; use Option<String> |  --> <input>:1:7 |   | | 1 | tex...
  saphyr options-only   ERR cannot deserialize null into string; use Option<String> at line 1, column 7
  saphyr RECOMMENDED    ERR cannot deserialize null into string; use Option<String> at line 1, column 7 [path: text]
  serde_yaml_ng         OK  Text { text: "~" }
  yaml_ng + pre-pass    OK  Text { text: "~" }
  marked-yaml           OK  Text { text: "~" }
### c/d "no"   text: "no"\n
  saphyr default        OK  Text { text: "no" }
  saphyr options-only   OK  Text { text: "no" }
  saphyr RECOMMENDED    OK  Text { text: "no" }
  serde_yaml_ng         OK  Text { text: "no" }
  yaml_ng + pre-pass    OK  Text { text: "no" }
  marked-yaml           OK  Text { text: "no" }
### c/d (empty value)   text:\n
  saphyr default        ERR error: line 1 column 5: cannot deserialize null into string; use Option<String> |  --> <input>:1:5 |   | | 1 | tex...
  saphyr options-only   ERR cannot deserialize null into string; use Option<String> at line 1, column 5
  saphyr RECOMMENDED    ERR cannot deserialize null into string; use Option<String> at line 1, column 5 [path: text]
  serde_yaml_ng         OK  Text { text: "" }
  yaml_ng + pre-pass    OK  Text { text: "" }
  marked-yaml           OK  Text { text: "" }
### d' Option<String> = ~   text: ~\n
  saphyr default        OK  MaybeText { text: None }
  saphyr options-only   OK  MaybeText { text: None }
  saphyr RECOMMENDED    OK  MaybeText { text: None }
  serde_yaml_ng         OK  MaybeText { text: None }
  yaml_ng + pre-pass    OK  MaybeText { text: None }
  marked-yaml           OK  MaybeText { text: Some("~") }
### e 1e3   n: 1e3\n
  saphyr default        ERR error: line 1 column 4: invalid i64 |  --> <input>:1:4 |   | | 1 | n: 1e3 |   |    ^ invalid i64
  saphyr options-only   ERR invalid i64 at line 1, column 4
  saphyr RECOMMENDED    ERR invalid i64 at line 1, column 4 [path: n]
  serde_yaml_ng         ERR n: invalid type: floating point `1000.0`, expected i64 at line 1 column 4
  yaml_ng + pre-pass    ERR n: invalid type: floating point `1000.0`, expected i64 at line 1 column 4
  marked-yaml           ERR n: invalid digit found in string @ line:col 1:4
### e 0x10   n: 0x10\n
  saphyr default        OK  Num { n: 16 }
  saphyr options-only   OK  Num { n: 16 }
  saphyr RECOMMENDED    OK  Num { n: 16 }
  serde_yaml_ng         OK  Num { n: 16 }
  yaml_ng + pre-pass    OK  Num { n: 16 }
  marked-yaml           ERR n: invalid digit found in string @ line:col 1:4
### e 1_000   n: 1_000\n
  saphyr default        OK  Num { n: 1000 }
  saphyr options-only   OK  Num { n: 1000 }
  saphyr RECOMMENDED    OK  Num { n: 1000 }
  serde_yaml_ng         ERR n: invalid type: string "1_000", expected i64 at line 1 column 4
  yaml_ng + pre-pass    ERR n: invalid type: string "1_000", expected i64 at line 1 column 4
  marked-yaml           ERR n: invalid digit found in string @ line:col 1:4
### e 010   n: 010\n
  saphyr default        ERR error: line 1 column 4: invalid i64 |  --> <input>:1:4 |   | | 1 | n: 010 |   |    ^ invalid i64
  saphyr options-only   ERR invalid i64 at line 1, column 4
  saphyr RECOMMENDED    ERR invalid i64 at line 1, column 4 [path: n]
  serde_yaml_ng         ERR n: invalid type: string "010", expected i64 at line 1 column 4
  yaml_ng + pre-pass    ERR n: invalid type: string "010", expected i64 at line 1 column 4
  marked-yaml           OK  Num { n: 10 }
### e' f64 = 1e3   x: 1e3\n
  saphyr default        OK  Real { x: 1000.0 }
  saphyr options-only   OK  Real { x: 1000.0 }
  saphyr RECOMMENDED    OK  Real { x: 1000.0 }
  serde_yaml_ng         OK  Real { x: 1000.0 }
  yaml_ng + pre-pass    OK  Real { x: 1000.0 }
  marked-yaml           OK  Real { x: 1000.0 }
### f1 struct field   a: 1\na: 2\n
  saphyr default        ERR error: line 2 column 1: duplicate mapping key: a, set DuplicateKeyPolicy in Options if acceptable |  --> <input>:2...
  saphyr options-only   ERR duplicate mapping key: a, set DuplicateKeyPolicy in Options if acceptable at line 2, column 1
  saphyr RECOMMENDED    ERR duplicate key `a` at line 2 column 1
  serde_yaml_ng         ERR duplicate field `a`
  yaml_ng + pre-pass    ERR duplicate key `a` at line 2 column 1
  marked-yaml           ERR Duplicate key "a" in mapping at 1:1 and 2:1
### f2 map key   k: 1\nk: 2\n
  saphyr default        ERR error: line 2 column 1: duplicate mapping key: k, set DuplicateKeyPolicy in Options if acceptable |  --> <input>:2...
  saphyr options-only   ERR duplicate mapping key: k, set DuplicateKeyPolicy in Options if acceptable at line 2, column 1
  saphyr RECOMMENDED    ERR duplicate key `k` at line 2 column 1
  serde_yaml_ng         OK  {"k": 2}
  yaml_ng + pre-pass    ERR duplicate key `k` at line 2 column 1
  marked-yaml           ERR Duplicate key "k" in mapping at 1:1 and 2:1
### f3 map key, flow style   {k: 1, k: 2}\n
  saphyr default        ERR error: line 1 column 8: duplicate mapping key: k, set DuplicateKeyPolicy in Options if acceptable |  --> <input>:1...
  saphyr options-only   ERR duplicate mapping key: k, set DuplicateKeyPolicy in Options if acceptable at line 1, column 8
  saphyr RECOMMENDED    ERR duplicate key `k` at line 1 column 8
  serde_yaml_ng         OK  {"k": 2}
  yaml_ng + pre-pass    ERR duplicate key `k` at line 1 column 8
  marked-yaml           ERR Duplicate key "k" in mapping at 1:2 and 1:8
### f4 k vs "k"   k: 1\n"k": 2\n
  saphyr default        ERR error: line 2 column 1: duplicate mapping key: k, set DuplicateKeyPolicy in Options if acceptable |  --> <input>:2...
  saphyr options-only   ERR duplicate mapping key: k, set DuplicateKeyPolicy in Options if acceptable at line 2, column 1
  saphyr RECOMMENDED    ERR duplicate key `k` at line 2 column 1
  serde_yaml_ng         OK  {"k": 2}
  yaml_ng + pre-pass    ERR duplicate key `k` at line 2 column 1
  marked-yaml           ERR Duplicate key "k" in mapping at 1:1 and 2:1
### g1   a: 1\nzzz: 2\n
  saphyr default        ERR error: line 2 column 1: unknown field `zzz`, expected one of a |  --> <input>:2:1 |   | | 1 | a: 1 | 2 | zzz: 2 | ...
  saphyr options-only   ERR unknown field `zzz`, expected one of a at line 2, column 1
  saphyr RECOMMENDED    ERR unknown field `zzz`, expected one of a at line 2, column 1 [path: zzz]
  serde_yaml_ng         ERR unknown field `zzz`, expected `a` at line 2 column 1
  yaml_ng + pre-pass    ERR unknown field `zzz`, expected `a` at line 2 column 1
  marked-yaml           ERR zzz: Unknown field `zzz`, expected `a` @ line:col 2:1
### g2 deny_unknown_fields + flatten   a: 1\nzzz: 2\n
  saphyr default        ERR unknown field `zzz`
  saphyr options-only   ERR unknown field `zzz`
  saphyr RECOMMENDED    ERR unknown field `zzz` [path: .]
  serde_yaml_ng         ERR unknown field `zzz`
  yaml_ng + pre-pass    ERR unknown field `zzz`
  marked-yaml           ERR unknown field `zzz` @ line:col 1:2
### h1 inline   <<: {a: 1}\nb: 2\n
  saphyr default        OK  Pair { a: 1, b: 2 }
  saphyr options-only   ERR YAML merge keys are not allowed by configured policy at line 1, column 1
  saphyr RECOMMENDED    ERR merge key not allowed at line 1 column 1
  serde_yaml_ng         ERR unknown field `<<`, expected `a` or `b`
  yaml_ng + pre-pass    ERR merge key not allowed at line 1 column 1
  marked-yaml           ERR <<: Unknown field `<<`, expected `a` or `b` @ line:col 1:1
### h2 with anchor   x: &d {a: 1}\ny:\n  <<: *d\n  b: 2\n
  saphyr default        OK  {"x": {"a": 1}, "y": {"a": 1, "b": 2}}
  saphyr options-only   ERR budget breached: Anchors { anchors: 1 } at line 1, column 7
  saphyr RECOMMENDED    ERR anchor not allowed at line 1 column 7
  serde_yaml_ng         ERR y.<<: invalid type: map, expected i64 at line 1 column 4
  yaml_ng + pre-pass    ERR anchor not allowed at line 1 column 7
  marked-yaml           ERR 1:7: Unexpected definition of anchor
### i1 !!str   text: !!str 1\n
  saphyr default        OK  Text { text: "1" }
  saphyr options-only   OK  Text { text: "1" }
  saphyr RECOMMENDED    ERR tag not allowed at line 1 column 13
  serde_yaml_ng         OK  Text { text: "1" }
  yaml_ng + pre-pass    ERR tag not allowed at line 1 column 13
  marked-yaml           ERR 1:13: Unexpected use of YAML tag
### i2 !custom   text: !custom x\n
  saphyr default        OK  Text { text: "x" }
  saphyr options-only   ERR unsupported tag `!custom` at line 1, column 15
  saphyr RECOMMENDED    ERR tag not allowed at line 1 column 15
  serde_yaml_ng         OK  Text { text: "x" }
  yaml_ng + pre-pass    ERR tag not allowed at line 1 column 15
  marked-yaml           ERR 1:15: Unexpected use of YAML tag
### i3 !!int   n: !!int 5\n
  saphyr default        OK  Num { n: 5 }
  saphyr options-only   OK  Num { n: 5 }
  saphyr RECOMMENDED    ERR tag not allowed at line 1 column 10
  serde_yaml_ng         OK  Num { n: 5 }
  yaml_ng + pre-pass    ERR tag not allowed at line 1 column 10
  marked-yaml           ERR 1:10: Unexpected use of YAML tag
### enum1 single-key map   list:\n  - draw: { count: 2 }\n  - pass\n
  saphyr default        OK  Steps { list: [Draw { count: 2 }, Pass] }
  saphyr options-only   OK  Steps { list: [Draw { count: 2 }, Pass] }
  saphyr RECOMMENDED    OK  Steps { list: [Draw { count: 2 }, Pass] }
  serde_yaml_ng         ERR list[0]: invalid type: map, expected a YAML tag starting with '!' at line 2 column 5
  yaml_ng + pre-pass    ERR list[0]: invalid type: map, expected a YAML tag starting with '!' at line 2 column 5
  marked-yaml           OK  Steps { list: [Draw { count: 2 }, Pass] }
### enum2 YAML tags   list:\n  - !draw { count: 2 }\n  - !pass\n
  saphyr default        OK  Steps { list: [Draw { count: 2 }, Pass] }
  saphyr options-only   ERR unsupported tag `!draw` at line 2, column 11
  saphyr RECOMMENDED    ERR tag not allowed at line 2 column 11
  serde_yaml_ng         OK  Steps { list: [Draw { count: 2 }, Pass] }
  yaml_ng + pre-pass    ERR tag not allowed at line 2 column 11
  marked-yaml           ERR 2:11: Unexpected use of YAML tag
### enum3 map style + singleton_map_recursive   list:\n  - draw: { count: 2 }\n  - pass\n
  saphyr default        OK  StepsAdapter { list: [Draw { count: 2 }, Pass] }
  saphyr options-only   OK  StepsAdapter { list: [Draw { count: 2 }, Pass] }
  saphyr RECOMMENDED    OK  StepsAdapter { list: [Draw { count: 2 }, Pass] }
  serde_yaml_ng         OK  StepsAdapter { list: [Draw { count: 2 }, Pass] }
  yaml_ng + pre-pass    OK  StepsAdapter { list: [Draw { count: 2 }, Pass] }
  marked-yaml           OK  StepsAdapter { list: [Draw { count: 2 }, Pass] }
### loc1 wrong type   zones:\n  - { id: deck, ordered: true }\n  - { id: hand, ordered: maybe }\n
  saphyr default        ERR error: line 3 column 26: invalid boolean |  --> <input>:3:26 |   | | 1 | zones: | 2 |   - { id: deck, ordered: tru...
  saphyr options-only   ERR invalid boolean (strict mode expects true/false) at line 3, column 26
  saphyr RECOMMENDED    ERR invalid boolean (strict mode expects true/false) at line 3, column 26 [path: zones[1].ordered]
  serde_yaml_ng         ERR zones[1].ordered: invalid type: string "maybe", expected a boolean at line 3 column 26
  yaml_ng + pre-pass    ERR zones[1].ordered: invalid type: string "maybe", expected a boolean at line 3 column 26
  marked-yaml           ERR zones[1].ordered: Value was not a boolean @ line:col 3:26
### loc2 unknown field   zones:\n  - { id: deck, ordered: true }\n  - { id: hand, ordered: false, extra: 1 }\n
  saphyr default        ERR error: line 3 column 33: unknown field `extra`, expected one of id, ordered |  --> <input>:3:33 |   | | 1 | zones:...
  saphyr options-only   ERR unknown field `extra`, expected one of id, ordered at line 3, column 33
  saphyr RECOMMENDED    ERR unknown field `extra`, expected one of id, ordered at line 3, column 33 [path: zones[1].extra]
  serde_yaml_ng         ERR zones[1]: unknown field `extra`, expected `id` or `ordered` at line 3 column 33
  yaml_ng + pre-pass    ERR zones[1]: unknown field `extra`, expected `id` or `ordered` at line 3 column 33
  marked-yaml           ERR zones[1].extra: Unknown field `extra`, expected `id` or `ordered` @ line:col 3:33
### loc3 syntax error   zones:\n  - { id: deck, ordered: true\n
  saphyr default        ERR error: line 2 column 5: unclosed bracket '{' |  --> <input>:2:5 |   | | 1 | zones: | 2 |   - { id: deck, ordered: ...
  saphyr options-only   ERR unclosed bracket '{' at line 2, column 5
  saphyr RECOMMENDED    ERR syntax error: unclosed bracket '{' at char 11 line 2 column 5
  serde_yaml_ng         ERR did not find expected ',' or '}' at line 3 column 1, while parsing a flow mapping at line 2 column 5
  yaml_ng + pre-pass    ERR syntax error: unclosed bracket '{' at char 11 line 2 column 5
  marked-yaml           ERR 3:1: while parsing a flow mapping, did not find expected ',' or '}'
### loc4 inside an enum variant   list:\n  - pass\n  - draw: { count: two }\n
  saphyr default        ERR error: line 3 column 20: invalid u32 |  --> <input>:3:20 |   | | 1 | list: | 2 |   - pass | 3 |   - draw: { count:...
  saphyr options-only   ERR invalid u32 at line 3, column 20
  saphyr RECOMMENDED    ERR invalid u32 at line 3, column 20 [path: list[1].draw.count]
  serde_yaml_ng         ERR list[1]: invalid type: map, expected a YAML tag starting with '!' at line 3 column 5
  yaml_ng + pre-pass    ERR list[1]: invalid type: map, expected a YAML tag starting with '!' at line 3 column 5
  marked-yaml           ERR list[1].draw.count: invalid digit found in string @ line:col 3:20
### shape1 newtype/tuple variants   list:\n  - fixed: 3\n  - range: [1, 2]\n
  saphyr default        OK  Amounts { list: [Fixed(3), Range(1, 2)] }
  saphyr options-only   OK  Amounts { list: [Fixed(3), Range(1, 2)] }
  saphyr RECOMMENDED    OK  Amounts { list: [Fixed(3), Range(1, 2)] }
  serde_yaml_ng         ERR list[0]: invalid type: map, expected a YAML tag starting with '!' at line 2 column 5
  yaml_ng + pre-pass    ERR list[0]: invalid type: map, expected a YAML tag starting with '!' at line 2 column 5
  marked-yaml           OK  Amounts { list: [Fixed(3), Range(1, 2)] }
### shape2 internally tagged enum   op: draw\ncount: 2\n
  saphyr default        OK  Draw { count: 2 }
  saphyr options-only   OK  Draw { count: 2 }
  saphyr RECOMMENDED    OK  Draw { count: 2 }
  serde_yaml_ng         OK  Draw { count: 2 }
  yaml_ng + pre-pass    OK  Draw { count: 2 }
  marked-yaml           ERR invalid type: string "2", expected u32 @ line:col 1:3
### shape4a untagged Bool|Int|Map, bool   init: false\n
  saphyr default        OK  InitDoc { init: Bool(false) }
  saphyr options-only   OK  InitDoc { init: Bool(false) }
  saphyr RECOMMENDED    OK  InitDoc { init: Bool(false) }
  serde_yaml_ng         OK  InitDoc { init: Bool(false) }
  yaml_ng + pre-pass    OK  InitDoc { init: Bool(false) }
  marked-yaml           ERR init: data did not match any variant of untagged enum Init @ line:col 1:7
### shape4b untagged Bool|Int|Map, map   init: { from_field: life }\n
  saphyr default        OK  InitDoc { init: FromField { from_field: "life" } }
  saphyr options-only   OK  InitDoc { init: FromField { from_field: "life" } }
  saphyr RECOMMENDED    OK  InitDoc { init: FromField { from_field: "life" } }
  serde_yaml_ng         OK  InitDoc { init: FromField { from_field: "life" } }
  yaml_ng + pre-pass    OK  InitDoc { init: FromField { from_field: "life" } }
  marked-yaml           OK  InitDoc { init: FromField { from_field: "life" } }
### shape3 flatten with integer fields   a: 1\nb: 2\n
  saphyr default        OK  FlatInt { a: 1, inner: IntInner { b: 2 } }
  saphyr options-only   OK  FlatInt { a: 1, inner: IntInner { b: 2 } }
  saphyr RECOMMENDED    OK  FlatInt { a: 1, inner: IntInner { b: 2 } }
  serde_yaml_ng         OK  FlatInt { a: 1, inner: IntInner { b: 2 } }
  yaml_ng + pre-pass    OK  FlatInt { a: 1, inner: IntInner { b: 2 } }
  marked-yaml           ERR invalid type: string "2", expected i64 @ line:col 1:2
### span Spanned<T>   id: deck\nsteps:\n  - move: { to: hand }\n
  saphyr default        OK  Located { id: Spanned { value: "deck", referenced: Location { line: 1, column: 5, span: Span { offset: 4, len: 4, ...
  saphyr options-only   OK  Located { id: Spanned { value: "deck", referenced: Location { line: 1, column: 5, span: Span { offset: 4, len: 4, ...
  saphyr RECOMMENDED    OK  Located { id: Spanned { value: "deck", referenced: Location { line: 1, column: 5, span: Span { offset: 4, len: 4, ...
  serde_yaml_ng         ERR steps[0]: invalid type: map, expected a YAML tag starting with '!' at line 3 column 5
  yaml_ng + pre-pass    ERR steps[0]: invalid type: map, expected a YAML tag starting with '!' at line 3 column 5
  marked-yaml           ERR id: invalid type: string "deck", expected a span-aware newtype wrapper @ line:col 1:5
### serde_norway output differed from serde_yaml_ng in 0 of 42 cases
### robust undefined alias
  marked-yaml           ERR 1:4: while parsing node, found unknown anchor
  saphyr RECOMMENDED    ERR syntax error: while parsing node, found unknown anchor at char 3 line 1 column 4
### robust two documents
  marked-yaml           OK  Known { a: 1 }
  saphyr RECOMMENDED    ERR multiple YAML documents detected; use from_str_multiple or from_str_multiple_with_options at line 3, column 1 [pat...
### robust empty file
  marked-yaml           ERR missing field `a` @ line:col 1:1
  saphyr RECOMMENDED    ERR unexpected end of input at line 1, column 1 [path: .]
### robust BOM + CRLF
  marked-yaml           ERR ﻿a: Unknown field `﻿a`, expected `a` @ line:col 1:1
  saphyr RECOMMENDED    OK  Known { a: 1 }
### robust 5000 nested [
  marked-yaml           ERR 1:256: recursion limit exceeded
  saphyr RECOMMENDED    ERR syntax error: recursion limit exceeded at char 255 line 1 column 256
### span (marked_yaml::Spanned)
  marked-yaml           OK  LocatedM { id: Spanned { span: Span { start: Some(Marker { source: 0, character: 4, line: 1, column: 5 }), end: No...
### real file /home/hgg/projects/tcg-engine/rulesets/small-ruleset/small-ruleset.yaml
  pre-pass            Ok(())
  saphyr RECOMMENDED (data ignored) Ok("OK")
  marked-yaml        (data ignored) Ok("OK")
```

### 5.5 Output of `tree` (verbatim)

```text

### anchor + alias
yaml: a: &x 1\nb: *x\n
  saphyr 0.3.3     OK  [Mapping({Value(String("a")): Value(Integer(1)), Value(String("b")): Value(Integer(1))})]
  yaml-rust2 0.13  OK  [Hash({String("a"): Integer(1), String("b"): Integer(1)})]

### duplicate key
yaml: k: 1\nk: 2\n
  saphyr 0.3.3     ERR duplicated key in mapping at byte 5 line 2 column 1
  yaml-rust2 0.13  ERR String("k"): duplicated key in mapping at byte 8 line 2 column 4

### merge key
yaml: <<: {a: 1}\nb: 2\n
  saphyr 0.3.3     OK  [Mapping({Value(String("<<")): Mapping({Value(String("a")): Value(Integer(1))}), Value(String("b")): Value(Integer(2))})]
  yaml-rust2 0.13  OK  [Hash({String("<<"): Hash({String("a"): Integer(1)}), String("b"): Integer(2)})]

### tags
yaml: a: !!str 1\nb: !custom x\n
  saphyr 0.3.3     OK  [Mapping({Value(String("a")): Value(String("1")), Value(String("b")): Tagged(Tag { handle: "!", suffix: "custom" }, Value(String("x")))})]
  yaml-rust2 0.13  OK  [Hash({String("a"): String("1"), String("b"): String("x")})]

### implicit typing: yes no on ~ 1e3 0x1F 010 null
yaml: a: yes\nb: no\nc: on\nd: ~\ne: 1e3\nf: 0x1F\ng: 010\nh: null\ni: True\n
  saphyr 0.3.3     OK  [Mapping({Value(String("a")): Value(String("yes")), Value(String("b")): Value(String("no")), Value(String("c")): Value(String("on")), Value(String("d")): Value(Null), Value(String("e")): Value(FloatingPoint(1000.0)), Value(String("f")): Value(Integer(31)), Value(String("g")): Value(Integer(10)), Value(String("h")): Value(Null), Value(String("i")): Value(Boolean(true))})]
  yaml-rust2 0.13  OK  [Hash({String("a"): String("yes"), String("b"): String("no"), String("c"): String("on"), String("d"): Null, String("e"): Real("1e3"), String("f"): Integer(31), String("g"): Integer(10), String("h"): Null, String("i"): Boolean(true)})]

### quoted forms stay strings
yaml: a: "yes"\nb: '~'\nc: '1e3'\n
  saphyr 0.3.3     OK  [Mapping({Value(String("a")): Value(String("yes")), Value(String("b")): Value(String("~")), Value(String("c")): Value(String("1e3"))})]
  yaml-rust2 0.13  OK  [Hash({String("a"): String("yes"), String("b"): String("~"), String("c"): String("1e3")})]
```
