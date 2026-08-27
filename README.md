# brouhaha

[![CI](https://github.com/dp88/brouhaha/actions/workflows/ci.yml/badge.svg)](https://github.com/dp88/brouhaha/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/brouhaha.svg)](https://crates.io/crates/brouhaha)
[![docs.rs](https://img.shields.io/docsrs/brouhaha)](https://docs.rs/brouhaha)
![MSRV](https://img.shields.io/badge/rust-1.88%2B-blue)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

A type-driven combat kernel for old-school basic/expert fantasy rules: the
whole side-based combat round — declarations, initiative, morale, movement,
missiles, magic, melee — as a `no_std` Rust library where illegal states do
not compile.

## Quick start

```toml
[dependencies]
brouhaha = "0.1"
```

```rust
use brouhaha::prelude::*;
use brouhaha::weapon::stock;

let mut dice = SeededDice::seeded(48317);
let scores = Abilities::flat(AbilityScore::new(13).unwrap());
let hero = AdventurerSheet::new("Aldra", &FIGHTER, Level::new(3).unwrap(), scores)
    .armour(Armour::Medium)
    .shield()
    .weapon(stock::sword())
    .build(&mut dice)
    .unwrap();

let boar_kind = MonsterKind::new(
    "boar",
    HitDice::of(2),
    ArmourClass::new(12),
    NonEmpty::of(MonsterAttack::melee("tusk", DiceExpr::of(1, Die::D6))),
);
let boar = Combatant::monster(&boar_kind, &mut dice);

let mut builder = CombatBuilder::new(Rules::default());
let heroes = builder.side();
let beasts = builder.side();
let a = builder.join(heroes, hero);
let b = builder.join(beasts, boar);
let mut field = AbstractField::new();
field.set_distance(a, b, Feet(5));
field.set_engaged(a, true);
field.set_engaged(b, true);

let combat = builder.begin().unwrap();
let mut melee = combat
    .roll_initiative(&mut dice)
    .finish_morale()
    .finish_movement()
    .finish_missiles()
    .finish_magic();
let attacker = melee.acting_members()[0];
let target = if attacker == a { b } else { a };
let strike = melee.witness_melee(attacker, target, &field).unwrap();
melee.strike(strike, &mut dice).unwrap();
assert!(!melee.log().all().is_empty());
```

## Why

- **Illegal states do not compile.** The round is a typestate pipeline:
  `Combat<Declaring>` has no `strike`, and a missile shot at a target behind
  total cover cannot even be constructed. Evidence types
  (`MeleeTarget`, `MissileTarget`, `LegalMove`) prove a rule check happened.
- **Your world, your grid.** The kernel asks a four-method `SpatialOracle`
  for distance, sight, cover, and engagement. Use the built-in
  theatre-of-the-mind field, the optional [spacewalk] adapter, or your own.
- **Commands in, events out.** Every fact lands in one typed event log;
  a recorded command list plus a dice seed replays a fight exactly.
- **Extensible classes.** Four classic classes ship as data-backed `Class`
  implementations; a custom class is one `impl` reusing the public
  progression tables, with combat hooks for abilities like back-stab.
- **All the optional rules**, as strategy enums, never booleans: variable
  weapon damage, individual initiative, slow weapons, morale, missile
  reload, simultaneous initiative ties, and surprise.

[spacewalk]: https://github.com/dp88/spacewalk

## Requirements and features

- Rust 1.88 or newer, edition 2024.
- `no_std` with `alloc`; no required dependencies.
- `serde` feature: serialization for commands, events, and the rules
  configuration — persistence by replay.
- `spacewalk` feature: a `SpatialOracle` over any `spacewalk::Grid`, with
  feet-to-cells scaling and pathfinding-backed movement evidence.

## More examples and documentation

- [API documentation](https://docs.rs/brouhaha) — rustdoc is the manual.
- [`tests/`](tests/) — scenario tests: duels, kiting, spell disruption,
  morale breaks, grid fights, command replay.
- [CHANGELOG](CHANGELOG.md)

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

This library implements game mechanics — procedures and mathematics — with
original text and code. It reproduces no copyrighted text and uses no
trademarks.
