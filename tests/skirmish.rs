//! Scenarios: movement allowances, missile fire, cover, and retreat.

use brouhaha::prelude::*;
use brouhaha::weapon::stock;

/// A roller that returns a scripted sequence, for exact-threshold tests.
struct Script(Vec<u8>);

impl DiceRoller for Script {
    fn roll(&mut self, _die: Die) -> u8 {
        self.0.remove(0)
    }
}

fn scores(all: u8) -> Abilities {
    Abilities::flat(AbilityScore::new(all).unwrap())
}

/// An archer: level 1 fighter, DEX 13 (+1 missile), long bow, no armour.
fn archer(dice: &mut dyn DiceRoller) -> Combatant {
    AdventurerSheet::new("archer", &FIGHTER, Level::ONE, scores(13))
        .weapon(stock::long_bow())
        .build(dice)
        .unwrap()
}

/// A shieldman: level 1 fighter, all 9s, light armour and shield (AC 13).
fn shieldman(dice: &mut dyn DiceRoller) -> Combatant {
    AdventurerSheet::new("shieldman", &FIGHTER, Level::ONE, scores(9))
        .armour(Armour::Light)
        .shield()
        .weapon(stock::sword())
        .build(dice)
        .unwrap()
}

struct Setup {
    combat: Combat<Declaring>,
    a: CombatantId,
    b: CombatantId,
    field: AbstractField,
}

/// Side 0 holds `a` (the archer), side 1 holds `b` (the shieldman).
fn setup(distance: Feet) -> Setup {
    let mut sheet_dice = SeededDice::seeded(99);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let a = builder.join(s0, archer(&mut sheet_dice));
    let b = builder.join(s1, shieldman(&mut sheet_dice));
    let mut field = AbstractField::new();
    field.set_distance(a, b, distance);
    Setup {
        combat: builder.begin().unwrap(),
        a,
        b,
        field,
    }
}

/// Roll initiative so side 0 acts first (script: 6 then 1), and advance to
/// the missile stage.
fn to_missiles(combat: Combat<Declaring>, dice: &mut Script) -> Combat<MissileStage> {
    dice.0.splice(0..0, [6, 1]);
    combat
        .roll_initiative(dice)
        .finish_morale()
        .finish_movement()
}

#[test]
fn missile_thresholds_follow_band_and_dexterity() {
    // AC 13; archer bonus +0, DEX +1, short range +1: a roll of 11 hits.
    let Setup {
        combat,
        a,
        b,
        field,
    } = setup(Feet(60));
    let mut dice = Script(vec![11, 4]);
    let mut stage = to_missiles(combat, &mut dice);
    let shot = stage.witness_shot(a, b, &field).unwrap();
    assert_eq!(shot.band(), RangeBand::Short);
    stage.shoot(shot, &mut dice).unwrap();
    let hit = stage.log().all().iter().any(|e| {
        matches!(
            e,
            CombatEvent::AttackRolled {
                hit: true,
                total: 13,
                ..
            }
        )
    });
    assert!(hit, "11 + 1 dex + 1 short = 13 hits AC 13");
    assert!(
        stage
            .log()
            .all()
            .iter()
            .any(|e| matches!(e, CombatEvent::DamageDealt { amount: 4, .. })),
        "flat d6 damage, unmodified by dexterity"
    );

    // Long range trades +1 for -1: a roll of 11 now misses (11 + 1 - 1 = 11).
    let Setup {
        combat,
        a,
        b,
        field,
    } = setup(Feet(200));
    let mut dice = Script(vec![11]);
    let mut stage = to_missiles(combat, &mut dice);
    let shot = stage.witness_shot(a, b, &field).unwrap();
    assert_eq!(shot.band(), RangeBand::Long);
    stage.shoot(shot, &mut dice).unwrap();
    assert!(stage.log().all().iter().any(|e| matches!(
        e,
        CombatEvent::AttackRolled {
            hit: false,
            total: 11,
            ..
        }
    )));
}

#[test]
fn missiles_respect_range_reach_cover_and_engagement() {
    // Beyond the long bow's 210 feet: no shot exists.
    let Setup {
        combat,
        a,
        b,
        field,
    } = setup(Feet(211));
    let stage = to_missiles(combat, &mut Script(vec![]));
    assert_eq!(
        stage.witness_shot(a, b, &field),
        Err(TargetingError::OutOfRange)
    );

    // Within five feet: too close for missiles.
    let Setup {
        combat,
        a,
        b,
        field,
    } = setup(Feet(5));
    let stage = to_missiles(combat, &mut Script(vec![]));
    assert_eq!(
        stage.witness_shot(a, b, &field),
        Err(TargetingError::TooCloseForMissiles)
    );

    // Total cover: untargetable.
    let Setup {
        combat,
        a,
        b,
        mut field,
    } = setup(Feet(60));
    field.set_cover(a, b, Cover::Total);
    let stage = to_missiles(combat, &mut Script(vec![]));
    assert_eq!(
        stage.witness_shot(a, b, &field),
        Err(TargetingError::TotalCover)
    );

    // Partial cover applies its penalty: -4 turns the hit into a miss.
    let Setup {
        combat,
        a,
        b,
        mut field,
    } = setup(Feet(60));
    field.set_cover(a, b, Cover::Partial(CoverPenalty::new(4).unwrap()));
    let mut dice = Script(vec![11]);
    let mut stage = to_missiles(combat, &mut dice);
    let shot = stage.witness_shot(a, b, &field).unwrap();
    stage.shoot(shot, &mut dice).unwrap();
    assert!(stage.log().all().iter().any(|e| matches!(
        e,
        CombatEvent::AttackRolled {
            hit: false,
            total: 9,
            ..
        }
    )));

    // No line of sight: no shot.
    let Setup {
        combat,
        a,
        b,
        mut field,
    } = setup(Feet(60));
    field.block_sight(a, b);
    let stage = to_missiles(combat, &mut Script(vec![]));
    assert_eq!(
        stage.witness_shot(a, b, &field),
        Err(TargetingError::NoLineOfSight)
    );

    // An engaged archer cannot fire.
    let Setup {
        combat,
        a,
        b,
        mut field,
    } = setup(Feet(60));
    field.set_engaged(a, true);
    let stage = to_missiles(combat, &mut Script(vec![]));
    assert_eq!(
        stage.witness_shot(a, b, &field),
        Err(TargetingError::AttackerEngaged)
    );
}

#[test]
fn movement_allowances_follow_commitments() {
    // Free movement: up to the encounter rate (120 / 3 = 40 feet).
    let Setup {
        combat,
        a,
        b: _,
        field,
    } = setup(Feet(60));
    let mut dice = Script(vec![6, 1]);
    let stage = combat.roll_initiative(&mut dice).finish_morale();
    assert!(stage.witness_move(a, Feet(40), &field).is_ok());
    assert_eq!(
        stage.witness_move(a, Feet(41), &field),
        Err(MoveError::TooFar)
    );

    // Engaged without a declaration: no free movement.
    let Setup {
        combat,
        a,
        b: _,
        mut field,
    } = setup(Feet(5));
    field.set_engaged(a, true);
    let mut dice = Script(vec![6, 1]);
    let stage = combat.roll_initiative(&mut dice).finish_morale();
    assert_eq!(
        stage.witness_move(a, Feet(10), &field),
        Err(MoveError::EngagedInMelee)
    );

    // A declared withdrawal moves at up to half the encounter rate.
    let Setup {
        mut combat,
        a,
        b: _,
        mut field,
    } = setup(Feet(5));
    field.set_engaged(a, true);
    combat.declare_withdrawal(a, &field).unwrap();
    let mut dice = Script(vec![6, 1]);
    let mut stage = combat.roll_initiative(&mut dice).finish_morale();
    assert_eq!(
        stage.witness_move(a, Feet(21), &field),
        Err(MoveError::TooFar)
    );
    let mv = stage.witness_move(a, Feet(20), &field).unwrap();
    stage.make_move(mv);
    assert!(stage.log().all().iter().any(|e| matches!(
        e,
        CombatEvent::Moved {
            kind: MoveKind::Withdrawal,
            distance: Feet(20),
            ..
        }
    )));
    // One move per round.
    assert_eq!(
        stage.witness_move(a, Feet(10), &field),
        Err(MoveError::AlreadyMoved)
    );

    // A declared retreat moves at the full encounter rate.
    let Setup {
        mut combat,
        a,
        b: _,
        mut field,
    } = setup(Feet(5));
    field.set_engaged(a, true);
    combat.declare_retreat(a, &field).unwrap();
    let mut dice = Script(vec![6, 1]);
    let stage = combat.roll_initiative(&mut dice).finish_morale();
    assert!(stage.witness_move(a, Feet(40), &field).is_ok());

    // Declaring either without being engaged is refused.
    let Setup {
        mut combat,
        a,
        b: _,
        field,
    } = setup(Feet(60));
    assert_eq!(
        combat.declare_retreat(a, &field),
        Err(DeclareError::NotEngaged)
    );
}

#[test]
fn a_retreating_target_is_easier_to_hit_and_loses_the_shield() {
    // Two shieldmen in melee; `b` declares a full retreat.
    let mut sheet_dice = SeededDice::seeded(3);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let a = builder.join(s0, shieldman(&mut sheet_dice));
    let b = builder.join(s1, shieldman(&mut sheet_dice));
    let mut field = AbstractField::new();
    field.set_distance(a, b, Feet(5));
    field.set_engaged(a, true);
    field.set_engaged(b, true);

    let mut combat = builder.begin().unwrap();
    combat.declare_retreat(b, &field).unwrap();

    // b's AC is 13; retreating drops the shield (12) and grants +2 to be
    // hit. A roll of 10 (total 12) therefore hits.
    let mut dice = Script(vec![6, 1, 10, 3]);
    let mut melee = combat
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = melee.witness_melee(a, b, &field).unwrap();
    melee.strike(strike, &mut dice).unwrap();
    assert!(melee.log().all().iter().any(|e| matches!(
        e,
        CombatEvent::AttackRolled {
            hit: true,
            total: 12,
            ..
        }
    )));

    // The retreater may not attack on its own turn.
    let TurnEnd::NextGroup(next) = melee.finish_melee() else {
        panic!("the fight goes on");
    };
    let their_melee = next
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    assert_eq!(
        their_melee.witness_melee(b, a, &field),
        Err(TargetingError::CommittedElsewhere)
    );
}
