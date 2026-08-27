//! Scenario: a fight driven through the flat command boundary, recorded,
//! and replayed exactly.

use brouhaha::prelude::*;
use brouhaha::weapon::stock;

fn fighter(name: &str, dice: &mut SeededDice) -> Combatant {
    AdventurerSheet::new(
        name,
        &FIGHTER,
        Level::new(2).unwrap(),
        Abilities::flat(AbilityScore::new(13).unwrap()),
    )
    .armour(Armour::Light)
    .weapon(stock::sword())
    .build(dice)
    .unwrap()
}

fn build(
    seed: u64,
) -> (
    AnyCombat,
    CombatantId,
    CombatantId,
    AbstractField,
    SeededDice,
) {
    let mut dice = SeededDice::seeded(seed);
    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let a = builder.join(s0, fighter("Aldra", &mut dice));
    let b = builder.join(s1, fighter("Berek", &mut dice));
    let mut field = AbstractField::new();
    field.set_distance(a, b, Feet(5));
    field.set_engaged(a, true);
    field.set_engaged(b, true);
    (
        AnyCombat::Declaring(builder.begin().unwrap()),
        a,
        b,
        field,
        dice,
    )
}

#[test]
fn a_command_driven_fight_records_and_replays_exactly() {
    let (mut combat, a, b, field, mut dice) = build(48317);
    let mut recording = CommandLog::new();

    // Drive to a conclusion: each phase either strikes or finishes.
    let mut guard = 0;
    while !combat.is_over() {
        guard += 1;
        assert!(guard < 1000, "the fight must end");
        let command = match combat.phase() {
            "declaring" => Command::RollInitiative,
            "melee" => {
                // Try a strike for each combatant; fall back to finishing.
                let mut struck = None;
                for (attacker, target) in [(a, b), (b, a)] {
                    let cmd = Command::Strike { attacker, target };
                    match combat.apply(&cmd, &field, &mut dice) {
                        Ok((next, _)) => {
                            combat = next;
                            recording.push(cmd);
                            struck = Some(());
                            break;
                        }
                        Err((next, _)) => combat = next,
                    }
                }
                if struck.is_some() {
                    continue;
                }
                Command::FinishStage
            }
            _ => Command::FinishStage,
        };
        combat = match combat.apply(&command, &field, &mut dice) {
            Ok((next, _)) => {
                recording.push(command);
                next
            }
            Err((next, e)) => panic!("command refused: {e:?} in {}", next.phase()),
        };
    }
    let original: Vec<String> = combat
        .log()
        .all()
        .iter()
        .map(|e| format!("{e:?}"))
        .collect();
    assert!(original.iter().any(|e| e.starts_with("CombatEnded")));

    // Replay the recording against a fresh combat with the same seeds.
    let (fresh, _, _, field, mut dice) = build(48317);
    let replayed = fresh.replay(recording.commands(), &field, &mut dice);
    let echoed: Vec<String> = replayed
        .log()
        .all()
        .iter()
        .map(|e| format!("{e:?}"))
        .collect();
    assert_eq!(original, echoed);
    assert!(replayed.is_over());
}

#[test]
fn a_refused_command_returns_the_combat_unchanged() {
    let (combat, a, b, field, mut dice) = build(7);
    // Striking during declarations is the wrong phase.
    let cmd = Command::Strike {
        attacker: a,
        target: b,
    };
    let Err((combat, CommandError::WrongPhase)) = combat.apply(&cmd, &field, &mut dice) else {
        panic!("the strike must be refused");
    };
    assert_eq!(combat.phase(), "declaring");
    assert!(
        combat.log().all().len() == 1,
        "nothing was logged but the round start"
    );
}

#[cfg(feature = "serde")]
#[test]
fn commands_and_events_round_trip_through_serde() {
    let (combat, a, b, field, mut dice) = build(3);
    let Ok((combat, _)) = combat.apply(&Command::RollInitiative, &field, &mut dice) else {
        panic!("initiative is legal while declaring");
    };

    let strike = Command::Strike {
        attacker: a,
        target: b,
    };
    let json = serde_json::to_string(&strike).unwrap();
    let back: Command = serde_json::from_str(&json).unwrap();
    assert_eq!(back, strike);

    let events = serde_json::to_string(combat.log().all()).unwrap();
    assert!(events.contains("SideInitiative"));

    // Validated newtypes stay validated: a d20 face of 21 is refused.
    assert!(serde_json::from_str::<NaturalRoll>("21").is_err());
    assert!(serde_json::from_str::<NaturalRoll>("20").is_ok());
}
