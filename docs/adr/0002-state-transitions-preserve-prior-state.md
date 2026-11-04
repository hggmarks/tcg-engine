# State transitions preserve the prior GameState

Each accepted Action produces a distinct next GameState and ordered Event Trace while leaving the input GameState unchanged; rejected Actions produce a typed error and no events. This supports branching deterministic simulations and makes the transition contract explicit without committing to a particular memory representation.

**Status**: accepted
