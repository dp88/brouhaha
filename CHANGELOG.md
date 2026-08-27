# Changelog

## Unreleased

## 0.1.0 — 2026-08-26

Initial release.

- Typestate combat round: declarations, initiative, morale, movement,
  missiles, magic, melee.
- Validated rules data: ability scores and modifiers, ascending armour
  class (with descending and THAC0 conversions), attack and save
  resolution, class progression tables, monster hit dice and experience.
- Four built-in classes (fighter, cleric, magic-user, thief) behind an
  extensible `Class` trait with `ClassAbility` combat hooks; back-stab and
  turn undead implemented through the public hooks.
- Effect algebra for spells, monster attack riders, and special actions,
  with a `CustomEffect` escape hatch.
- Optional rules as strategies: variable weapon damage, individual
  initiative, slow weapons, morale, missile reload, simultaneous ties,
  surprise.
- `SpatialOracle` world boundary with targeting evidence types and a
  theatre-of-the-mind `AbstractField`.
- `AnyCombat` + `Command` flat driver with exact replay from a command
  list and a dice seed.
- `serde` feature for the command/event boundary; `spacewalk` feature for
  grid support.
