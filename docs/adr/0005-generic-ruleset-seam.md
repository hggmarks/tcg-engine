# The Runtime is generic over each Ruleset's own types

The Runtime is generic over a `Ruleset` trait whose associated State, Action, Event and Rejection types belong to each Ruleset. Its `transition` takes the GameState and the Action by shared reference and returns either a new GameState with its Event Trace or a typed Rejection. Game creation and legal-Action listing are not part of the seam, because creation inputs differ per Ruleset. The Phase B declarative Ruleset has a completely different GameState shape, so it and the Toy Ruleset can run side by side without reworking the Toy, and ADR-0003 stays true for the Toy. This refines ADR-0001 and ADR-0002.

Conflict flagged with ADR-0004: ADR-0004 makes game rulesets declarative data and says the engine knows only the typed model it calls `Ruleset`. Both still hold. The Toy Ruleset is a proving scenario written in code. Game Rulesets stay declarative data per ADR-0004, and they will run through this same trait as a declarative Ruleset that interprets their typed model ([07]). The trait keeps the name `Ruleset`, as in ADR-0001 and CONTEXT.md, so ADR-0004's typed model needs another name; ticket #19 records it as `RulesetDefinition`.

**Status**: accepted
