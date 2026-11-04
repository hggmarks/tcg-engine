# YAML loader crate and engine-owned validation

Only the `tcg-loader` crate parses YAML, and only into a plain `RulesetDraft` (strings for ids, no checks). It uses `serde-saphyr` pinned to `=2.1.0` with the strict options from `docs/research/yaml-parser.md`, plus an event pass that rejects anchors, aliases and every tag, including `!!str`. The engine has no serde or YAML dependency and performs every semantic check in `RulesetDefinition::validate`, so a YAML, code or script frontend gets identical errors. Loading is stage 1 of ADR-0004's two-stage loading; the catalog stage will take the resulting `&RulesetDefinition`.

The first error wins, in a fixed order: `parameters.players`, `parameters.starting_player`, then zones, card types and inputs, each list in declaration order; within an element, its id is checked for duplicates before its content. Semantic errors carry an id-addressed path (`card_types[creature].fields[cost]`); duplicate-id errors address the two elements by index. Syntax errors carry the positional path (`card_types[1].fields[0]`) plus line and column. Path keys are the format's key names whatever the frontend. Id namespaces are per list: a zone and an input may share an id.

`init` is either a scalar or a `{ from_field: <name> }` map, which no derived `Deserialize` loads. One hand-written `Deserialize` reads it, accepts bool, integer (fitting in `i64`) and string scalars, and keeps the scalar's YAML type. The engine then checks it strictly against the declared field type, without coercion: `type: text, init: 5` and `type: bool, init: "false"` are mismatches. Making `init` always a map would change the file format and is left to the user.

Accepted residuals: `True` and `TRUE` read as booleans, and integers may be written `0x10` or `1_000`.

**Status**: accepted
