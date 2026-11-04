# The Runtime is generic over each Ruleset's own types

The Runtime is generic over a `Ruleset` trait whose associated State, Action, Event and Rejection types belong to each Ruleset. Its `transition` takes the GameState and the Action by shared reference and returns either a new GameState with its Event Trace or a typed Rejection. Game creation and legal-Action listing are not part of the seam, because creation inputs differ per Ruleset. The Phase B declarative Ruleset has a completely different GameState shape, so it and the Toy Ruleset can run side by side without reworking the Toy, and ADR-0003 stays true for the Toy. This refines ADR-0001 and ADR-0002 and conflicts with neither.

**Status**: accepted
