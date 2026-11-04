# Ruleset owns game legality and transitions

The generic Runtime coordinates state transitions but does not encode game-specific legality or mutation rules. A substitutable Ruleset receives the current GameState and Action, validates game legality, and calculates the next GameState and normative Event Trace; this preserves game agnosticism without prematurely introducing effects or plugin infrastructure.

**Status**: accepted
