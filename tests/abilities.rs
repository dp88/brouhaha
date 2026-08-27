//! Scenarios: class abilities through the public hooks, and a custom class.

use brouhaha::prelude::*;
use brouhaha::weapon::stock;

struct Script(Vec<u8>);

impl DiceRoller for Script {
    fn roll(&mut self, _die: Die) -> u8 {
        self.0.remove(0)
    }
}

fn scores(all: u8) -> Abilities {
    Abilities::flat(AbilityScore::new(all).unwrap())
}

#[test]
fn back_stab_gains_four_and_doubles_damage_against_the_unaware() {
    // A thief ambushes a surprised swordsman in light armour (AC 12).
    let mut builder = CombatBuilder::new(Rules::default());
    let sneaks = builder.side();
    let marks = builder.side();
    let thief = builder.join(
        sneaks,
        AdventurerSheet::new("thief", &THIEF, Level::ONE, scores(9))
            .weapon(stock::dagger())
            .build(&mut Script(vec![4]))
            .unwrap(),
    );
    let mark = builder.join(
        marks,
        AdventurerSheet::new("mark", &FIGHTER, Level::ONE, scores(9))
            .armour(Armour::Light)
            .weapon(stock::sword())
            .build(&mut Script(vec![8]))
            .unwrap(),
    );
    builder.surprised(marks);
    let mut field = AbstractField::new();
    field.set_distance(thief, mark, Feet(5));

    // A roll of 8 misses AC 12 on its own; back-stab's +4 turns it into a
    // hit. The d6 damage roll of 3 doubles to 6.
    let mut dice = Script(vec![6, 8, 3]);
    let mut melee = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = melee.witness_melee(thief, mark, &field).unwrap();
    melee.strike(strike, &mut dice).unwrap();

    let log = melee.log().all();
    assert!(log.iter().any(|e| matches!(
        e,
        CombatEvent::AttackRolled {
            hit: true,
            total: 12,
            ..
        }
    )));
    assert!(
        log.iter()
            .any(|e| matches!(e, CombatEvent::DamageDealt { amount: 6, .. }))
    );
}

#[test]
fn back_stab_does_not_apply_to_the_aware() {
    // The same numbers without surprise: the 8 misses.
    let mut builder = CombatBuilder::new(Rules::default());
    let sneaks = builder.side();
    let marks = builder.side();
    let thief = builder.join(
        sneaks,
        AdventurerSheet::new("thief", &THIEF, Level::ONE, scores(9))
            .weapon(stock::dagger())
            .build(&mut Script(vec![4]))
            .unwrap(),
    );
    let mark = builder.join(
        marks,
        AdventurerSheet::new("mark", &FIGHTER, Level::ONE, scores(9))
            .armour(Armour::Light)
            .weapon(stock::sword())
            .build(&mut Script(vec![8]))
            .unwrap(),
    );
    let mut field = AbstractField::new();
    field.set_distance(thief, mark, Feet(5));

    let mut dice = Script(vec![6, 1, 8]);
    let mut melee = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = melee.witness_melee(thief, mark, &field).unwrap();
    melee.strike(strike, &mut dice).unwrap();
    assert!(melee.log().all().iter().any(|e| matches!(
        e,
        CombatEvent::AttackRolled {
            hit: false,
            total: 8,
            ..
        }
    )));
}

fn skeleton(hp_roll: u8) -> Combatant {
    let kind = MonsterKind::new(
        "rattlebones",
        HitDice::of(1),
        ArmourClass::new(12),
        NonEmpty::of(MonsterAttack::melee("claw", DiceExpr::of(1, Die::D6))),
    );
    Combatant::monster(&kind, &mut Script(vec![hp_roll]))
}

#[test]
fn turn_undead_routs_the_lesser_dead_and_a_high_cleric_destroys_them() {
    // A first-level cleric turns two one-die skeletons: 2d6 = 8 beats the
    // needed 7, and the 2d6 pool of 6 covers both.
    let mut builder = CombatBuilder::new(Rules::default());
    let faithful = builder.side();
    let shamblers = builder.side();
    let cleric = builder.join(
        faithful,
        AdventurerSheet::new("cleric", &CLERIC, Level::ONE, scores(9))
            .weapon(stock::mace())
            .build(&mut Script(vec![4]))
            .unwrap(),
    );
    let s1 = builder.join(shamblers, skeleton(5));
    let s2 = builder.join(shamblers, skeleton(5));
    let field = AbstractField::new();

    let mut dice = Script(vec![6, 1, 4, 4, 3, 3]);
    let mut magic = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles();
    let turn = TurnUndead {
        cleric_level: Level::ONE,
        undead: vec![(s1, HitDice::of(1), false), (s2, HitDice::of(1), false)],
    };
    magic.special(cleric, &turn, &mut dice).unwrap();
    assert!(
        magic
            .combatant(s1)
            .unwrap()
            .has_condition(Condition::Fleeing)
    );
    assert!(
        magic
            .combatant(s2)
            .unwrap()
            .has_condition(Condition::Fleeing)
    );
    // Turning replaces the cleric's attack.
    let melee = magic.finish_magic();
    assert_eq!(
        melee.witness_melee(cleric, s1, &field),
        Err(TargetingError::AlreadyAttacked)
    );
    // Both skeletons routed: the combat is over.
    let TurnEnd::Over(done) = melee.finish_melee() else {
        panic!("the rout ends the fight");
    };
    assert_eq!(done.standing(), vec![faithful]);

    // A fourth-level cleric destroys one-die undead outright: no attempt
    // roll threshold, and the pool of 4 covers both.
    let mut builder = CombatBuilder::new(Rules::default());
    let faithful = builder.side();
    let shamblers = builder.side();
    let cleric = builder.join(
        faithful,
        AdventurerSheet::new("cleric", &CLERIC, Level::new(4).unwrap(), scores(9))
            .weapon(stock::mace())
            .build(&mut Script(vec![4, 4, 4, 4]))
            .unwrap(),
    );
    let s1 = builder.join(shamblers, skeleton(5));
    let s2 = builder.join(shamblers, skeleton(5));

    let mut dice = Script(vec![6, 1, 2, 2, 2, 2]);
    let mut magic = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles();
    let turn = TurnUndead {
        cleric_level: Level::new(4).unwrap(),
        undead: vec![(s1, HitDice::of(1), false), (s2, HitDice::of(1), false)],
    };
    magic.special(cleric, &turn, &mut dice).unwrap();
    assert!(!magic.combatant(s1).unwrap().is_alive());
    assert!(!magic.combatant(s2).unwrap().is_alive());
    let _ = cleric;
}

// ---------------------------------------------------------------------------
// A custom class, defined entirely outside the crate.
// ---------------------------------------------------------------------------

/// A defensive knack: +2 to armour class while aware of the attacker.
struct Wardstance;

const WARDSTANCE: Wardstance = Wardstance;

impl ClassAbility for Wardstance {
    fn name(&self) -> &str {
        "wardstance"
    }

    fn modify_defence(&self, ctx: &AttackContext<'_>) -> Modifier {
        if ctx.target_unaware() {
            Modifier::ZERO
        } else {
            Modifier::new(2)
        }
    }
}

/// A sturdy delver: fighter tables, level cap 12, any gear, wardstance.
struct Delver;

impl Class for Delver {
    fn name(&self) -> &str {
        "delver"
    }

    fn hit_die(&self) -> Die {
        Die::D8
    }

    fn max_level(&self) -> Level {
        Level::new(12).unwrap()
    }

    fn attack_bonus(&self, level: Level) -> AttackBonus {
        brouhaha::class::MARTIAL_ATTACK.at(level)
    }

    fn saves(&self, level: Level) -> SavingThrowProfile {
        brouhaha::class::MARTIAL_SAVES.at(level)
    }

    fn abilities(&self) -> &[&'static dyn ClassAbility] {
        &[&WARDSTANCE]
    }
}

#[test]
fn a_custom_class_plugs_in_with_its_own_ability() {
    // The delver's level cap holds.
    let err = AdventurerSheet::new("d", &Delver, Level::new(13).unwrap(), scores(9))
        .weapon(stock::sword())
        .build(&mut Script(vec![4]))
        .unwrap_err();
    assert_eq!(err, SheetError::LevelAboveClassMax);

    // Wardstance raises the effective armour class by two: a roll of 12
    // would hit AC 12, but the delver deflects it.
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let foe = builder.join(
        s0,
        AdventurerSheet::new("foe", &FIGHTER, Level::ONE, scores(9))
            .weapon(stock::sword())
            .build(&mut Script(vec![8]))
            .unwrap(),
    );
    let delver = builder.join(
        s1,
        AdventurerSheet::new("delver", &Delver, Level::ONE, scores(9))
            .armour(Armour::Light)
            .weapon(stock::sword())
            .build(&mut Script(vec![8]))
            .unwrap(),
    );
    let mut field = AbstractField::new();
    field.set_distance(foe, delver, Feet(5));

    let mut dice = Script(vec![6, 1, 12, 14, 3]);
    let mut melee = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = melee.witness_melee(foe, delver, &field).unwrap();
    melee.strike(strike, &mut dice).unwrap();
    assert!(
        melee.log().all().iter().any(|e| matches!(
            e,
            CombatEvent::AttackRolled {
                hit: false,
                total: 12,
                ..
            }
        )),
        "12 misses the warded AC 14"
    );
    // A fresh witness allows a second strike attempt? No: one attack per
    // round. The 14 in the script goes unused by this attacker.
    assert_eq!(
        melee.witness_melee(foe, delver, &field),
        Err(TargetingError::AlreadyAttacked)
    );
}
