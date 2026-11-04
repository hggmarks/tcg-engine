# Ruleset Definition names the typed model

*Ruleset* keeps its ADR-0001 and CONTEXT.md meaning: the behavioral seam the Runtime calls. The typed data model that ADR-0004 calls `Ruleset` is named `RulesetDefinition`, and `CardCatalog` and `DeckList` keep the names ADR-0004 gave them. One word for two concepts would make code and docs ambiguous: "the Ruleset validates an Action" and "the Ruleset has three zones" would describe different things.

This amends decision 3 of ADR-0004 (the typed-model names) and nothing else in it. It does not conflict with ADR-0001.

**Status**: accepted
