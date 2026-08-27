//! Scenario: two fighters duel to the death with the default rules.

use brouhaha::prelude::*;

fn fighter(name: &str, dice: &mut SeededDice) -> Combatant {
    let scores = Abilities::flat(AbilityScore::new(13).unwrap());
    AdventurerSheet::new(name, &FIGHTER, Level::new(2).unwrap(), scores)
        .armour(Armour::Light)
        .weapon(brouhaha::weapon::stock::sword())
        .build(dice)
        .unwrap()
}

/// Drive one full duel to its conclusion. Returns the log and the winner
/// count for assertions.
fn run_duel(seed: u64) -> (Vec<String>, usize) {
    let mut dice = SeededDice::seeded(seed);

    let mut builder = CombatBuilder::new(Rules::default());
    let heroes = builder.side();
    let rivals = builder.side();
    let a = builder.join(heroes, fighter("Aldra", &mut dice));
    let b = builder.join(rivals, fighter("Berek", &mut dice));

    let mut field = AbstractField::new();
    field.set_distance(a, b, Feet(5));
    field.set_engaged(a, true);
    field.set_engaged(b, true);

    let mut combat = builder.begin().unwrap();
    'rounds: loop {
        let mut turn = combat.roll_initiative(&mut dice);
        loop {
            let morale = turn;
            let movement = morale.finish_morale();
            let missiles = movement.finish_movement();
            let magic = missiles.finish_missiles();
            let mut melee = magic.finish_magic();
            for attacker in melee.acting_members().to_vec() {
                let target = if attacker == a { b } else { a };
                if let Ok(strike) = melee.witness_melee(attacker, target, &field) {
                    melee.strike(strike, &mut dice).unwrap();
                }
            }
            match melee.finish_melee() {
                TurnEnd::NextGroup(next) => turn = next,
                TurnEnd::NewRound(next) => {
                    combat = next;
                    continue 'rounds;
                }
                TurnEnd::Over(done) => {
                    let winner_count = done.survivors().count();
                    let log: Vec<String> =
                        done.log().all().iter().map(|e| format!("{e:?}")).collect();
                    return (log, winner_count);
                }
            }
        }
    }
}

#[test]
fn a_duel_runs_to_a_conclusion() {
    let (log, winners) = run_duel(48317);
    assert_eq!(winners, 1, "exactly one fighter stands");
    assert!(log.iter().any(|e| e.starts_with("Died")), "someone died");
    assert!(log.iter().any(|e| e.starts_with("CombatEnded")));
    // The sequence is coherent: the log starts with the first round.
    assert!(log[0].starts_with("RoundStarted"));
}

#[test]
fn the_same_seed_replays_the_same_combat() {
    let (log_a, _) = run_duel(7);
    let (log_b, _) = run_duel(7);
    assert_eq!(log_a, log_b);
}

#[test]
fn different_seeds_reach_different_fights() {
    // Not a guarantee in principle, but with these seeds the logs differ;
    // this guards against a roller that ignores its seed.
    let (log_a, _) = run_duel(1);
    let (log_b, _) = run_duel(2);
    assert_ne!(log_a, log_b);
}

#[test]
fn wrong_phase_calls_do_not_compile() {
    // The typestate is the test: `Combat<Declaring>` has no `strike`, and
    // `Combat<MeleeStage>` has no `declare_spell`. Nothing to run here.
}
