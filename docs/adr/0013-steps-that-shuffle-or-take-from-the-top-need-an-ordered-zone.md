# Steps that shuffle or take from the Top need an ordered zone

Definition validation now enforces a zone's `ordered` flag, which until now was only stored. The `zone` of a setup `shuffle`, the `from` of a setup `draw`, and the `from` of a turn `draw` or `move` must name a zone declared `ordered: true`; otherwise validation fails with `ZoneNotOrdered { zone }` at the path of that key (`setup[0].shuffle.zone`, `turn.phases[start].automatic[2].else[0].draw.from`), after the reference was checked to exist. A destination may be unordered: the cards a draw puts into the hand, or a move into the Mana Zone, have no order to keep. This is a user decision on #22, and it amends ADR-0011, which only required the zones of setup steps to exist.

A DeckList does not record the ruleset id and version it was validated against: the trust of ADR-0011 stays, and creation re-checks only what it needs in order never to panic.

The same change renames the error `InvalidDrawCount` to `InvalidCount`, because it now covers the `count` of a draw and of a move.

**Status**: accepted
