# Typed PlayerState owns Player domain facts

Player-owned gameplay facts are represented by a typed PlayerState aligned one-to-one with the immutable Player Order, rather than a generic metadata bag. The first such fact is a signed LifeTotal; this keeps invariants explicit and observable while allowing negative life without introducing terminal-outcome semantics.

*Amended by ADR-0041: this typed PlayerState is the Toy Ruleset's. The declarative Ruleset has a generic counterpart, Player state declared by the ruleset's `player_state` and changed through `PlayerStateChanged`; "rather than a generic metadata bag" still holds, since each value is a typed Player-State Schema and the engine names none.*

**Status**: accepted

Amended by ADR-0041: a generic, ruleset-declared Player state exists in the declarative Ruleset (marked above); the typed LifeTotal stays in the Toy Ruleset.
