//! Scenarios: morale, surprise, and the optional rules.

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

/// A level 1 fighter with the given weapon. Sheet dice control the hit
/// points exactly: one d8 roll.
fn fighter(name: &str, weapon: Weapon, hp_roll: u8, str_score: u8) -> Combatant {
    let mut abilities = scores(9);
    abilities.strength = AbilityScore::new(str_score).unwrap();
    AdventurerSheet::new(name, &FIGHTER, Level::ONE, abilities)
        .weapon(weapon)
        .build(&mut Script(vec![hp_roll]))
        .unwrap()
}

/// A small biter: one hit die minus one, bite d6, morale as given.
fn biter(morale: u8, hp_roll: u8) -> Combatant {
    let mut kind = MonsterKind::new(
        "biter",
        HitDice::plus(1, -1),
        ArmourClass::new(11),
        NonEmpty::of(MonsterAttack::melee("bite", DiceExpr::of(1, Die::D6))),
    );
    kind.morale = Morale::new(morale).unwrap();
    Combatant::monster(&kind, &mut Script(vec![hp_roll]))
}

#[test]
fn casualties_force_morale_checks_and_a_break_routs_the_side() {
    let mut builder = CombatBuilder::new(Rules::default());
    let heroes = builder.side();
    let pack = builder.side();
    let hero = builder.join(heroes, fighter("hero", stock::sword(), 8, 13));
    let g1 = builder.join(pack, biter(7, 5));
    let g2 = builder.join(pack, biter(7, 5));
    let mut field = AbstractField::new();
    for other in [g1, g2] {
        field.set_distance(hero, other, Feet(5));
    }
    field.set_distance(g1, g2, Feet(5));

    let combat = builder.begin().unwrap();
    // Heroes win initiative; the hero fells the first biter (15 + 1 STR
    // hits armour class 11; damage 6 + 1 kills 4 hit points).
    let mut dice = Script(vec![6, 1, 15, 6]);
    let mut melee = combat
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = melee.witness_melee(hero, g1, &field).unwrap();
    melee.strike(strike, &mut dice).unwrap();
    assert!(!melee.combatant(g1).unwrap().is_alive());

    // The pack's turn opens with morale. One death out of two triggers
    // both the first-casualty check and the half-strength check.
    let TurnEnd::NextGroup(pack_turn) = melee.finish_melee() else {
        panic!("the fight goes on");
    };
    let mut morale = pack_turn;
    assert_eq!(morale.due(), vec![pack]);
    // First check: 3 + 3 = 6 holds against morale 7.
    let mut dice = Script(vec![3, 3]);
    assert_eq!(morale.check(pack, &mut dice), Ok(MoraleOutcome::StandsFirm));
    // The half-strength check is still due: 6 + 6 = 12 breaks.
    assert_eq!(morale.due(), vec![pack]);
    let mut dice = Script(vec![6, 6]);
    assert_eq!(morale.check(pack, &mut dice), Ok(MoraleOutcome::Breaks));
    assert!(
        morale
            .combatant(g2)
            .unwrap()
            .has_condition(Condition::Fleeing)
    );
    assert!(
        morale
            .log()
            .all()
            .iter()
            .any(|e| matches!(e, CombatEvent::SideBroke { .. }))
    );

    // A routed side no longer stands; the combat ends.
    let melee = morale
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let TurnEnd::Over(done) = melee.finish_melee() else {
        panic!("the rout ends the fight");
    };
    assert_eq!(done.standing(), vec![heroes]);
    // Both biters are defeated: one slain, one routed. 5 XP each.
    assert_eq!(done.xp_award(heroes), 10);
}

#[test]
fn morale_two_breaks_without_a_roll_and_twelve_never_checks() {
    for (morale, breaks) in [(2, true), (12, false)] {
        let mut builder = CombatBuilder::new(Rules::default());
        let heroes = builder.side();
        let pack = builder.side();
        let hero = builder.join(heroes, fighter("hero", stock::sword(), 8, 13));
        let g1 = builder.join(pack, biter(morale, 5));
        let g2 = builder.join(pack, biter(morale, 5));
        let mut field = AbstractField::new();
        field.set_distance(hero, g1, Feet(5));
        field.set_distance(hero, g2, Feet(5));

        let combat = builder.begin().unwrap();
        let mut dice = Script(vec![6, 1, 15, 6]);
        let mut melee = combat
            .roll_initiative(&mut dice)
            .finish_morale()
            .finish_movement()
            .finish_missiles()
            .finish_magic();
        let strike = melee.witness_melee(hero, g1, &field).unwrap();
        melee.strike(strike, &mut dice).unwrap();
        let TurnEnd::NextGroup(pack_turn) = melee.finish_melee() else {
            panic!("the fight goes on");
        };
        let mut morale_stage = pack_turn;
        if breaks {
            // Morale 2 never fights: the due check breaks without a roll.
            assert_eq!(morale_stage.due(), vec![pack]);
            let mut no_dice = Script(vec![]);
            assert_eq!(
                morale_stage.check(pack, &mut no_dice),
                Ok(MoraleOutcome::Breaks)
            );
        } else {
            // Morale 12 is fearless: no check is ever due.
            assert!(morale_stage.due().is_empty());
        }
    }
}

#[test]
fn individual_initiative_orders_by_dexterity() {
    let rules = Rules {
        initiative: InitiativeRule::Individual,
        ..Rules::default()
    };
    let mut builder = CombatBuilder::new(rules);
    let s0 = builder.side();
    let s1 = builder.side();
    let mut nimble_scores = scores(9);
    nimble_scores.dexterity = AbilityScore::new(16).unwrap();
    let nimble = builder.join(
        s0,
        AdventurerSheet::new("nimble", &FIGHTER, Level::ONE, nimble_scores)
            .weapon(stock::sword())
            .build(&mut Script(vec![8]))
            .unwrap(),
    );
    let slouch = builder.join(s1, fighter("slouch", stock::sword(), 8, 9));

    // Both roll 4; +2 dexterity puts the nimble fighter first.
    let mut dice = Script(vec![4, 4]);
    let turn = builder.begin().unwrap().roll_initiative(&mut dice);
    assert_eq!(turn.acting_members(), &[nimble]);
    assert_eq!(turn.acting(), Some(s0));
    let _ = slouch;
}

#[test]
fn slow_weapons_act_last_when_the_rule_is_on() {
    let rules = Rules {
        slow_weapons: SlowWeaponRule::ActLast,
        ..Rules::default()
    };
    let mut builder = CombatBuilder::new(rules);
    let s0 = builder.side();
    let s1 = builder.side();
    let heavy = builder.join(s0, fighter("heavy", stock::two_handed_sword(), 8, 9));
    let quick = builder.join(s1, fighter("quick", stock::sword(), 8, 9));
    let mut field = AbstractField::new();
    field.set_distance(heavy, quick, Feet(5));

    // The heavy fighter's side wins initiative, but the great blade waits.
    let mut dice = Script(vec![6, 1]);
    let melee = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    assert_eq!(
        melee.witness_melee(heavy, quick, &field),
        Err(TargetingError::ActsLast)
    );

    // The quick fighter strikes on their own turn (a roll of 2 misses, so
    // nobody dies)...
    let TurnEnd::NextGroup(their_turn) = melee.finish_melee() else {
        panic!("the fight goes on");
    };
    let mut their_melee = their_turn
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = their_melee.witness_melee(quick, heavy, &field).unwrap();
    let mut dice = Script(vec![2]);
    their_melee.strike(strike, &mut dice).unwrap();

    // ...and the trailing slow group finally lets the great blade swing.
    let TurnEnd::NextGroup(last_turn) = their_melee.finish_melee() else {
        panic!("the slow group remains");
    };
    assert_eq!(last_turn.acting(), None);
    assert_eq!(last_turn.acting_members(), &[heavy]);
    let mut last_melee = last_turn
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = last_melee.witness_melee(heavy, quick, &field).unwrap();
    let mut dice = Script(vec![2]);
    last_melee.strike(strike, &mut dice).unwrap();
    assert!(matches!(last_melee.finish_melee(), TurnEnd::NewRound(_)));
}

#[test]
fn reloading_weapons_skip_every_other_round() {
    let rules = Rules {
        reload: ReloadRule::EveryOtherRound,
        ..Rules::default()
    };
    let mut builder = CombatBuilder::new(rules);
    let s0 = builder.side();
    let s1 = builder.side();
    let sniper = builder.join(s0, fighter("sniper", stock::crossbow(), 8, 9));
    let wall = builder.join(
        s1,
        AdventurerSheet::new("wall", &FIGHTER, Level::ONE, scores(9))
            .armour(Armour::Heavy)
            .weapon(stock::sword())
            .build(&mut Script(vec![8]))
            .unwrap(),
    );
    let mut field = AbstractField::new();
    field.set_distance(sniper, wall, Feet(60));

    // Round one: the crossbow fires (and misses on a 3).
    let mut dice = Script(vec![6, 1, 3]);
    let mut missiles = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement();
    let shot = missiles.witness_shot(sniper, wall, &field).unwrap();
    missiles.shoot(shot, &mut dice).unwrap();
    let TurnEnd::NextGroup(their_turn) = missiles.finish_missiles().finish_magic().finish_melee()
    else {
        panic!("the fight goes on");
    };
    let TurnEnd::NewRound(round_two) = their_turn
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic()
        .finish_melee()
    else {
        panic!("round two comes");
    };

    // Round two: reloading.
    let mut dice = Script(vec![6, 1]);
    let missiles = round_two
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement();
    assert_eq!(
        missiles.witness_shot(sniper, wall, &field),
        Err(TargetingError::NeedsReload)
    );
    let TurnEnd::NextGroup(their_turn) = missiles.finish_missiles().finish_magic().finish_melee()
    else {
        panic!("the fight goes on");
    };
    let TurnEnd::NewRound(round_three) = their_turn
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic()
        .finish_melee()
    else {
        panic!("round three comes");
    };

    // Round three: loaded again.
    let mut dice = Script(vec![6, 1]);
    let missiles = round_three
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement();
    assert!(missiles.witness_shot(sniper, wall, &field).is_ok());
}

#[test]
fn variable_weapon_damage_uses_the_weapons_dice() {
    let rules = Rules {
        damage: DamageRule::ByWeapon,
        ..Rules::default()
    };
    let mut builder = CombatBuilder::new(rules);
    let s0 = builder.side();
    let s1 = builder.side();
    let heavy = builder.join(s0, fighter("heavy", stock::two_handed_sword(), 8, 9));
    let tough = builder.join(s1, fighter("tough", stock::sword(), 8, 9));
    let mut field = AbstractField::new();
    field.set_distance(heavy, tough, Feet(5));

    // A natural 20 hits; the great blade rolls its own d10 for 9.
    let mut dice = Script(vec![6, 1, 20, 9]);
    let mut melee = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = melee.witness_melee(heavy, tough, &field).unwrap();
    melee.strike(strike, &mut dice).unwrap();
    assert!(
        melee
            .log()
            .all()
            .iter()
            .any(|e| matches!(e, CombatEvent::DamageDealt { amount: 9, .. }))
    );
}

#[test]
fn simultaneous_ties_let_both_sides_fall() {
    let rules = Rules {
        initiative: InitiativeRule::PerSide {
            ties: TieRule::Simultaneous,
        },
        ..Rules::default()
    };
    let mut builder = CombatBuilder::new(rules);
    let s0 = builder.side();
    let s1 = builder.side();
    // One hit point each: any hit kills.
    let a = builder.join(s0, fighter("a", stock::sword(), 1, 9));
    let b = builder.join(s1, fighter("b", stock::sword(), 1, 9));
    let mut field = AbstractField::new();
    field.set_distance(a, b, Feet(5));

    // Tied initiative: both sides act in one cluster. Both land natural
    // twenties; the damage lands together when the cluster ends.
    let mut dice = Script(vec![4, 4, 20, 6]);
    let mut melee = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = melee.witness_melee(a, b, &field).unwrap();
    melee.strike(strike, &mut dice).unwrap();
    // The blow is banked: b still stands, and still gets to swing.
    assert!(melee.combatant(b).unwrap().is_alive());
    let TurnEnd::NextGroup(their_turn) = melee.finish_melee() else {
        panic!("the tied cluster continues");
    };
    let mut their_melee = their_turn
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = their_melee.witness_melee(b, a, &field).unwrap();
    let mut dice = Script(vec![20, 6]);
    their_melee.strike(strike, &mut dice).unwrap();

    // The cluster ends: both blows land, both fighters fall, nobody stands.
    let TurnEnd::Over(done) = their_melee.finish_melee() else {
        panic!("mutual destruction ends the fight");
    };
    assert!(done.standing().is_empty());
    assert!(!done.combatant(a).unwrap().is_alive());
    assert!(!done.combatant(b).unwrap().is_alive());
}

#[test]
fn a_surprised_side_loses_the_first_round() {
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let hero = builder.join(s0, fighter("hero", stock::sword(), 8, 9));
    let lurker = builder.join(s1, fighter("lurker", stock::sword(), 8, 9));
    builder.surprised(s1);
    let field = AbstractField::new();

    let mut combat = builder.begin().unwrap();
    // The surprised side cannot declare.
    assert_eq!(
        combat.declare_retreat(lurker, &field),
        Err(DeclareError::Surprised)
    );

    // Only the alert side rolls and acts in round one.
    let mut dice = Script(vec![6]);
    let turn = combat.roll_initiative(&mut dice);
    assert_eq!(turn.acting(), Some(s0));
    assert_eq!(turn.acting_members(), &[hero]);
    let TurnEnd::NewRound(mut round_two) = turn
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic()
        .finish_melee()
    else {
        panic!("one group, then a new round");
    };

    // In round two the ambushers act normally.
    let field2 = {
        let mut f = AbstractField::new();
        f.set_distance(hero, lurker, Feet(5));
        f.set_engaged(hero, true);
        f.set_engaged(lurker, true);
        f
    };
    assert!(round_two.declare_retreat(lurker, &field2).is_ok());
    let mut dice = Script(vec![1, 6]);
    let turn = round_two.roll_initiative(&mut dice);
    assert_eq!(turn.acting(), Some(s1));
}
