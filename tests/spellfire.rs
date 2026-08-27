//! Scenarios: spell declaration, casting, disruption, and saves.

use brouhaha::magic::{SpellRange, SpellTargeting};
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

/// A bolt: level 1, one target in sight within 120 feet, d6+1 damage.
fn bolt() -> Spell {
    Spell {
        name: "bolt".into(),
        level: SpellLevel::new(1).unwrap(),
        range: SpellRange::Feet(Feet(120)),
        targeting: SpellTargeting::Single,
        needs_sight: true,
        magical: true,
        effect: Effect::Damage(DiceExpr::of(1, Die::D6).plus(1)),
    }
}

/// A hush: level 1, silences one target for a round, no save.
fn hush() -> Spell {
    Spell {
        name: "hush".into(),
        level: SpellLevel::new(1).unwrap(),
        range: SpellRange::Feet(Feet(120)),
        targeting: SpellTargeting::Single,
        needs_sight: true,
        magical: true,
        effect: Effect::Apply {
            condition: Condition::Silenced,
            duration: Duration::Rounds(Rounds(1)),
        },
    }
}

/// A blast: level 1, 2d6 damage, save versus spells for half.
fn blast() -> Spell {
    Spell {
        name: "blast".into(),
        level: SpellLevel::new(1).unwrap(),
        range: SpellRange::Feet(Feet(120)),
        targeting: SpellTargeting::Single,
        needs_sight: true,
        magical: true,
        effect: Effect::Save {
            category: SaveCategory::Spells,
            magical: true,
            mitigation: SaveMitigation::HalvesDamage,
            effect: Box::new(Effect::Damage(DiceExpr::of(2, Die::D6))),
        },
    }
}

fn caster(name: &str, spell: Spell, dice: &mut dyn DiceRoller) -> Combatant {
    let slots = MAGIC_USER.spell_slots(Level::ONE);
    AdventurerSheet::new(name, &MAGIC_USER, Level::ONE, scores(9))
        .weapon(stock::dagger())
        .spells(Memorized::prepare(slots, vec![spell]).unwrap())
        .build(dice)
        .unwrap()
}

fn archer(dice: &mut dyn DiceRoller) -> Combatant {
    AdventurerSheet::new("archer", &FIGHTER, Level::ONE, scores(9))
        .weapon(stock::long_bow())
        .build(dice)
        .unwrap()
}

fn first_spell(c: &Combatant) -> SpellRef {
    c.spells().unwrap().available().next().unwrap().0
}

#[test]
fn a_cast_spell_lands_and_spends_the_slot() {
    let mut sheet_dice = SeededDice::seeded(1);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let mu = builder.join(s0, caster("mu", bolt(), &mut sheet_dice));
    let foe = builder.join(s1, archer(&mut sheet_dice));
    let mut field = AbstractField::new();
    field.set_distance(mu, foe, Feet(60));

    let mut combat = builder.begin().unwrap();
    let spell = first_spell(combat.combatant(mu).unwrap());
    combat
        .declare_spell(mu, spell, DeclaredTarget::One(foe))
        .unwrap();

    // Casting wins initiative; the d6 damage roll is 4, so 4 + 1 = 5 lands.
    let mut dice = Script(vec![6, 1, 4]);
    let movement = combat.roll_initiative(&mut dice).finish_morale();
    // The committed caster cannot move...
    assert_eq!(
        movement.witness_move(mu, Feet(10), &field),
        Err(MoveError::CommittedToCasting)
    );
    let missiles = movement.finish_movement();
    // ...and cannot attack.
    assert_eq!(
        missiles.witness_shot(mu, foe, &field),
        Err(TargetingError::CommittedElsewhere)
    );
    let mut magic = missiles.finish_missiles();
    magic.cast(mu, &field, &mut dice).unwrap();

    let log = magic.log().all();
    assert!(
        log.iter()
            .any(|e| matches!(e, CombatEvent::SpellCast { .. }))
    );
    assert!(
        log.iter()
            .any(|e| matches!(e, CombatEvent::DamageDealt { amount: 5, .. }))
    );
    // The slot is spent.
    assert_eq!(
        magic
            .combatant(mu)
            .unwrap()
            .spells()
            .unwrap()
            .available()
            .count(),
        0
    );
    // Casting twice does not work.
    assert_eq!(
        magic.cast(mu, &field, &mut dice),
        Err(CastError::AlreadyCast)
    );
}

#[test]
fn a_hit_before_the_casters_turn_disrupts_and_loses_the_spell() {
    let mut sheet_dice = SeededDice::seeded(2);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let mu = builder.join(s0, caster("mu", bolt(), &mut sheet_dice));
    let foe = builder.join(s1, archer(&mut sheet_dice));
    let mut field = AbstractField::new();
    field.set_distance(mu, foe, Feet(60));

    let mut combat = builder.begin().unwrap();
    let spell = first_spell(combat.combatant(mu).unwrap());
    combat
        .declare_spell(mu, spell, DeclaredTarget::One(foe))
        .unwrap();

    // The archer wins initiative (1 versus 6) and hits the caster: a roll
    // of 15 plus short range +1 beats the caster's armour class 10.
    let mut dice = Script(vec![1, 6, 15, 3]);
    let mut missiles = combat
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement();
    let shot = missiles.witness_shot(foe, mu, &field).unwrap();
    missiles.shoot(shot, &mut dice).unwrap();

    // The spell is disrupted and lost before the caster's group acts.
    assert!(
        missiles
            .log()
            .all()
            .iter()
            .any(|e| matches!(e, CombatEvent::SpellDisrupted { .. }))
    );
    assert_eq!(
        missiles
            .combatant(mu)
            .unwrap()
            .spells()
            .unwrap()
            .available()
            .count(),
        0
    );

    // On the caster's turn, the cast fails as disrupted.
    let magic = missiles.finish_missiles();
    let TurnEnd::NextGroup(next) = magic.finish_magic().finish_melee() else {
        panic!("the fight goes on");
    };
    let mut their_magic = next.finish_morale().finish_movement().finish_missiles();
    assert_eq!(
        their_magic.cast(mu, &field, &mut dice),
        Err(CastError::Disrupted)
    );
}

#[test]
fn a_silenced_caster_keeps_the_spell() {
    // Two casters. The hush wins initiative and silences the bolt.
    let mut sheet_dice = SeededDice::seeded(3);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let bolt_mu = builder.join(s0, caster("bolt-mu", bolt(), &mut sheet_dice));
    let hush_mu = builder.join(s1, caster("hush-mu", hush(), &mut sheet_dice));
    let mut field = AbstractField::new();
    field.set_distance(bolt_mu, hush_mu, Feet(60));

    let mut combat = builder.begin().unwrap();
    let bolt_ref = first_spell(combat.combatant(bolt_mu).unwrap());
    let hush_ref = first_spell(combat.combatant(hush_mu).unwrap());
    combat
        .declare_spell(bolt_mu, bolt_ref, DeclaredTarget::One(hush_mu))
        .unwrap();
    combat
        .declare_spell(hush_mu, hush_ref, DeclaredTarget::One(bolt_mu))
        .unwrap();

    // Side 1 rolls 6 and acts first.
    let mut dice = Script(vec![1, 6]);
    let mut magic = combat
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles();
    magic.cast(hush_mu, &field, &mut dice).unwrap();
    assert!(
        magic
            .combatant(bolt_mu)
            .unwrap()
            .has_condition(Condition::Silenced)
    );

    // The silenced caster cannot cast, but the spell stays memorized.
    let TurnEnd::NextGroup(next) = magic.finish_magic().finish_melee() else {
        panic!("the fight goes on");
    };
    let mut their_magic = next.finish_morale().finish_movement().finish_missiles();
    assert_eq!(
        their_magic.cast(bolt_mu, &field, &mut dice),
        Err(CastError::CannotCast)
    );
    assert_eq!(
        their_magic
            .combatant(bolt_mu)
            .unwrap()
            .spells()
            .unwrap()
            .available()
            .count(),
        1
    );

    // The silence expires at the end of the round.
    let TurnEnd::NewRound(combat) = their_magic.finish_magic().finish_melee() else {
        panic!("the fight goes on");
    };
    assert!(
        !combat
            .combatant(bolt_mu)
            .unwrap()
            .has_condition(Condition::Silenced)
    );
}

#[test]
fn a_successful_save_halves_the_damage() {
    let mut sheet_dice = SeededDice::seeded(4);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let mu = builder.join(s0, caster("mu", blast(), &mut sheet_dice));
    let foe = builder.join(s1, archer(&mut sheet_dice));
    let mut field = AbstractField::new();
    field.set_distance(mu, foe, Feet(60));

    let mut combat = builder.begin().unwrap();
    let spell = first_spell(combat.combatant(mu).unwrap());
    combat
        .declare_spell(mu, spell, DeclaredTarget::One(foe))
        .unwrap();

    // Script: initiative 6/1, save roll 16 (fighter saves spells at 16:
    // success), damage 2d6 = 3 + 4 = 7, halved to 3.
    let mut dice = Script(vec![6, 1, 16, 3, 4]);
    let mut magic = combat
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles();
    magic.cast(mu, &field, &mut dice).unwrap();

    let log = magic.log().all();
    assert!(log.iter().any(|e| matches!(
        e,
        CombatEvent::SaveRolled {
            outcome: SaveOutcome::Success,
            ..
        }
    )));
    assert!(
        log.iter()
            .any(|e| matches!(e, CombatEvent::DamageDealt { amount: 3, .. }))
    );
}

#[test]
fn sight_and_range_gate_the_cast() {
    let mut sheet_dice = SeededDice::seeded(5);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let mu = builder.join(s0, caster("mu", bolt(), &mut sheet_dice));
    let foe = builder.join(s1, archer(&mut sheet_dice));
    let mut field = AbstractField::new();
    field.set_distance(mu, foe, Feet(60));
    field.block_sight(mu, foe);

    let mut combat = builder.begin().unwrap();
    let spell = first_spell(combat.combatant(mu).unwrap());
    combat
        .declare_spell(mu, spell, DeclaredTarget::One(foe))
        .unwrap();
    let mut dice = Script(vec![6, 1]);
    let mut magic = combat
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles();
    assert_eq!(
        magic.cast(mu, &field, &mut dice),
        Err(CastError::NoLineOfSight)
    );

    // Out of range: 121 feet beats the spell's 120.
    let mut sheet_dice = SeededDice::seeded(6);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let mu = builder.join(s0, caster("mu", bolt(), &mut sheet_dice));
    let foe = builder.join(s1, archer(&mut sheet_dice));
    let mut field = AbstractField::new();
    field.set_distance(mu, foe, Feet(121));
    let mut combat = builder.begin().unwrap();
    let spell = first_spell(combat.combatant(mu).unwrap());
    combat
        .declare_spell(mu, spell, DeclaredTarget::One(foe))
        .unwrap();
    let mut dice = Script(vec![6, 1]);
    let mut magic = combat
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles();
    assert_eq!(
        magic.cast(mu, &field, &mut dice),
        Err(CastError::OutOfRange)
    );
}
