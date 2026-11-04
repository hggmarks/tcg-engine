# Typed PlayerState owns Player domain facts

Player-owned gameplay facts are represented by a typed PlayerState aligned one-to-one with the immutable Player Order, rather than a generic metadata bag. The first such fact is a signed LifeTotal; this keeps invariants explicit and observable while allowing negative life without introducing terminal-outcome semantics.

**Status**: accepted
