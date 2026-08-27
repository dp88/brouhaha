//! Scenario: grid combat through the spacewalk adapter.
#![cfg(feature = "spacewalk")]

use brouhaha::grid::{CellScale, GridOracle, PathError, witness_path};
use brouhaha::prelude::*;
use brouhaha::weapon::stock;
use spacewalk::{Adjacency, FullGrid, Grid, Movement, Sq};

struct Script(Vec<u8>);

impl DiceRoller for Script {
    fn roll(&mut self, _die: Die) -> u8 {
        self.0.remove(0)
    }
}

fn scores(all: u8) -> Abilities {
    Abilities::flat(AbilityScore::new(all).unwrap())
}

fn fighter(name: &str, weapon: Weapon) -> Combatant {
    AdventurerSheet::new(name, &FIGHTER, Level::ONE, scores(9))
        .weapon(weapon)
        .build(&mut Script(vec![8]))
        .unwrap()
}

#[test]
fn a_pillar_blocks_sight_and_the_path_goes_around() {
    // An 8 by 8 board at five feet per cell. A pillar stands at (2, 1).
    let board = FullGrid::<Sq>::square(8, 8, Adjacency::Four);
    let scale = CellScale::FIVE;
    let pillar = board.at(Sq::new(2, 1));

    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let archer = builder.join(s0, fighter("archer", stock::long_bow()));
    let orc = builder.join(s1, fighter("orc", stock::sword()));

    // The archer stands at (0, 1); the orc at (4, 1), behind the pillar.
    let positions = [(archer, Sq::new(0, 1)), (orc, Sq::new(4, 1))];
    let roster = [(archer, s0), (orc, s1)];
    let position = |id: CombatantId| {
        positions
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, sq)| board.at(*sq))
    };
    let oracle = GridOracle::new(
        &board,
        scale,
        &roster,
        position,
        |i| i == pillar,
        |_, _| Cover::None,
    );

    // Distance: four cells is twenty feet.
    assert_eq!(oracle.distance(archer, orc), Some(Feet(20)));
    // Nobody is adjacent: no engagement.
    assert!(!oracle.engaged(archer));

    // The pillar blocks the shot.
    let mut dice = Script(vec![6, 1]);
    let combat = builder.begin().unwrap();
    let movement = combat.roll_initiative(&mut dice).finish_morale();

    // Walking straight through the pillar is impossible; the route bends
    // around it: six steps (thirty feet) instead of four.
    let walkable = Movement::cell_cost(&board, |c| if c == Sq::new(2, 1) { None } else { Some(1) });
    let (legal, path) = witness_path(
        &movement,
        archer,
        &oracle,
        &board,
        scale,
        board.at(Sq::new(0, 1)),
        board.at(Sq::new(4, 1)),
        &walkable,
    )
    .unwrap();
    assert_eq!(path.cost(), 6);
    assert_eq!(legal.distance(), Feet(30));
    assert!(!path.steps().contains(&pillar));

    // A destination beyond the forty-foot allowance is refused by the
    // rules, not by the pathfinder.
    let far = witness_path(
        &movement,
        archer,
        &oracle,
        &board,
        scale,
        board.at(Sq::new(0, 1)),
        board.at(Sq::new(7, 7)),
        &walkable,
    );
    assert!(matches!(far, Err(PathError::Move(MoveError::TooFar))));

    let missiles = movement.finish_movement();
    assert_eq!(
        missiles.witness_shot(archer, orc, &oracle),
        Err(TargetingError::NoLineOfSight)
    );
}

#[test]
fn adjacency_engages_and_allows_melee() {
    let board = FullGrid::<Sq>::square(8, 8, Adjacency::Four);
    let scale = CellScale::FIVE;

    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let hero = builder.join(s0, fighter("hero", stock::sword()));
    let orc = builder.join(s1, fighter("orc", stock::sword()));

    let positions = [(hero, Sq::new(3, 3)), (orc, Sq::new(3, 4))];
    let roster = [(hero, s0), (orc, s1)];
    let position = |id: CombatantId| {
        positions
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, sq)| board.at(*sq))
    };
    let oracle = GridOracle::new(
        &board,
        scale,
        &roster,
        position,
        |_| false,
        |_, _| Cover::None,
    );

    // One cell apart at five feet per cell: engaged, in melee reach.
    assert_eq!(oracle.distance(hero, orc), Some(Feet(5)));
    assert!(oracle.engaged(hero));
    assert!(oracle.engaged(orc));

    let mut dice = Script(vec![6, 1, 20, 4]);
    let mut melee = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    let strike = melee.witness_melee(hero, orc, &oracle).unwrap();
    melee.strike(strike, &mut dice).unwrap();
    assert!(
        melee
            .log()
            .all()
            .iter()
            .any(|e| matches!(e, CombatEvent::AttackRolled { hit: true, .. }))
    );
}

#[test]
fn the_ten_foot_scale_still_reaches_melee_next_cell() {
    // At ten feet per cell, five feet of reach still means one cell.
    let board = FullGrid::<Sq>::square(4, 4, Adjacency::Four);
    let scale = CellScale::TEN;

    let mut builder = CombatBuilder::new(Rules::default());
    let s0 = builder.side();
    let s1 = builder.side();
    let a = builder.join(s0, fighter("a", stock::sword()));
    let b = builder.join(s1, fighter("b", stock::sword()));
    let positions = [(a, Sq::new(0, 0)), (b, Sq::new(0, 1))];
    let roster = [(a, s0), (b, s1)];
    let position = |id: CombatantId| {
        positions
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, sq)| board.at(*sq))
    };
    let oracle = GridOracle::new(
        &board,
        scale,
        &roster,
        position,
        |_| false,
        |_, _| Cover::None,
    );

    assert!(oracle.engaged(a));
    // Adjacent cells count as melee reach at any scale, so coarse grids
    // still allow melee.
    assert_eq!(oracle.distance(a, b), Some(Feet(5)));
    let mut dice = Script(vec![6, 1]);
    let melee = builder
        .begin()
        .unwrap()
        .roll_initiative(&mut dice)
        .finish_morale()
        .finish_movement()
        .finish_missiles()
        .finish_magic();
    assert!(melee.witness_melee(a, b, &oracle).is_ok());
}
