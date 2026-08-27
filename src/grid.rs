//! Grid support: a [`SpatialOracle`] over any [`spacewalk::Grid`].
//!
//! Only present with the `spacewalk` feature. The application keeps its own
//! grid and position store; the adapter borrows both per query and converts
//! between cells and feet at exactly one place, [`CellScale`]. The kernel
//! never learns a cell existed.

use alloc::vec::Vec;

use spacewalk::{Cost, Grid, Idx, Movement, Path, Step};

use crate::combat::{Combat, MoveError, MovementStage};
use crate::space::{LegalMove, MELEE_REACH, SpatialOracle};
use crate::units::{CombatantId, Feet, SideId};
use crate::weapon::Cover;

/// Feet per grid cell. Five and ten feet are the book scales; any nonzero
/// scale is legal.
///
/// This is the only place cells and feet convert. `cells` rounds down: a
/// partial cell of allowance does not buy a step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellScale(Feet);

impl CellScale {
    /// Five feet per cell.
    pub const FIVE: CellScale = CellScale(Feet(5));

    /// Ten feet per cell.
    pub const TEN: CellScale = CellScale(Feet(10));

    /// Validate a scale. Returns `None` for zero feet per cell.
    pub const fn new(feet_per_cell: Feet) -> Option<CellScale> {
        if feet_per_cell.0 == 0 {
            None
        } else {
            Some(CellScale(feet_per_cell))
        }
    }

    /// The scale itself.
    pub const fn feet_per_cell(self) -> Feet {
        self.0
    }

    /// A cell count as feet, saturating at the `Feet` maximum.
    pub const fn feet(self, cells: u32) -> Feet {
        let feet = cells as u64 * self.0.0 as u64;
        if feet > u16::MAX as u64 {
            Feet(u16::MAX)
        } else {
            Feet(feet as u16)
        }
    }

    /// A distance in feet as whole cells, rounding down.
    pub const fn cells(self, distance: Feet) -> u32 {
        (distance.0 / self.0.0) as u32
    }
}

/// A [`SpatialOracle`] answering from a [`spacewalk::Grid`].
///
/// The application supplies the grid, the scale, the roster of combatants
/// with their sides, and three closures: where a combatant stands, which
/// cells block sight, and what cover one cell enjoys from another.
///
/// Return `None` from `position` for a combatant that is off the field —
/// including the dead, once the application removes them. An off-field
/// combatant cannot be targeted and engages nobody.
pub struct GridOracle<'a, G, P, B, C>
where
    G: Grid,
    P: Fn(CombatantId) -> Option<Idx>,
    B: Fn(Idx) -> bool,
    C: Fn(Idx, Idx) -> Cover,
{
    grid: &'a G,
    scale: CellScale,
    roster: &'a [(CombatantId, SideId)],
    position: P,
    blocks: B,
    cover: C,
}

impl<'a, G, P, B, C> GridOracle<'a, G, P, B, C>
where
    G: Grid,
    P: Fn(CombatantId) -> Option<Idx>,
    B: Fn(Idx) -> bool,
    C: Fn(Idx, Idx) -> Cover,
{
    /// Borrow the world for a batch of queries.
    pub fn new(
        grid: &'a G,
        scale: CellScale,
        roster: &'a [(CombatantId, SideId)],
        position: P,
        blocks: B,
        cover: C,
    ) -> Self {
        GridOracle {
            grid,
            scale,
            roster,
            position,
            blocks,
            cover,
        }
    }

    /// Melee reach in cells: at least one cell, whatever the scale.
    fn reach_cells(&self) -> u32 {
        self.scale.cells(MELEE_REACH).max(1)
    }
}

impl<G, P, B, C> SpatialOracle for GridOracle<'_, G, P, B, C>
where
    G: Grid,
    P: Fn(CombatantId) -> Option<Idx>,
    B: Fn(Idx) -> bool,
    C: Fn(Idx, Idx) -> Cover,
{
    fn distance(&self, a: CombatantId, b: CombatantId) -> Option<Feet> {
        let ia = (self.position)(a)?;
        let ib = (self.position)(b)?;
        let cells = self.grid.distance(ia, ib);
        let feet = self.scale.feet(cells);
        // Adjacent cells are melee reach at any scale: on a ten-foot grid,
        // fighters in neighbouring cells are toe to toe, not ten feet
        // apart. Without this, melee would be impossible on coarse grids.
        if cells <= self.reach_cells() && feet > MELEE_REACH {
            Some(MELEE_REACH)
        } else {
            Some(feet)
        }
    }

    fn line_of_sight(&self, a: CombatantId, b: CombatantId) -> bool {
        let (Some(ia), Some(ib)) = ((self.position)(a), (self.position)(b)) else {
            return false;
        };
        self.grid.los(ia, ib, &self.blocks)
    }

    fn cover(&self, attacker: CombatantId, target: CombatantId) -> Cover {
        let (Some(ia), Some(ib)) = ((self.position)(attacker), (self.position)(target)) else {
            return Cover::Total;
        };
        (self.cover)(ia, ib)
    }

    fn engaged(&self, who: CombatantId) -> bool {
        let Some(here) = (self.position)(who) else {
            return false;
        };
        let Some((_, my_side)) = self.roster.iter().find(|(id, _)| *id == who) else {
            return false;
        };
        let reach = self.reach_cells();
        self.roster.iter().any(|(other, side)| {
            *other != who
                && side != my_side
                && (self.position)(*other)
                    .is_some_and(|there| self.grid.distance(here, there) <= reach)
        })
    }
}

/// An error establishing a grid move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathError {
    /// No route exists between the cells under this movement.
    NoRoute,
    /// The route exists, but the rules refuse the move.
    Move(MoveError),
}

/// Establish a legal grid move: find a route, convert its cost to feet, and
/// validate it against the mover's allowance and declarations.
///
/// Movement costs count in cell-equivalents: a plain step costs one, and
/// rough ground may cost more, spending the allowance faster. On success
/// the caller gets the kernel's [`LegalMove`] evidence together with the
/// [`Path`] to animate; commit the new position to the application's own
/// store after [`Combat::make_move`].
#[allow(clippy::too_many_arguments)]
pub fn witness_path<G, F>(
    combat: &Combat<MovementStage>,
    who: CombatantId,
    oracle: &dyn SpatialOracle,
    grid: &G,
    scale: CellScale,
    from: Idx,
    to: Idx,
    movement: &Movement<F>,
) -> Result<(LegalMove, Path), PathError>
where
    G: Grid,
    F: Fn(Step<G::Cell>) -> Option<Cost>,
{
    let path = grid.path(from, to, movement).ok_or(PathError::NoRoute)?;
    let feet = scale.feet(path.cost());
    let legal = combat
        .witness_move(who, feet, oracle)
        .map_err(PathError::Move)?;
    Ok((legal, path))
}

/// The reachable cells this round: every destination whose route fits the
/// mover's remaining allowance, with its cost. A convenience over
/// [`spacewalk::Grid::reachable`] for movement previews.
pub fn reachable_this_round<G, F>(
    combat: &Combat<MovementStage>,
    who: CombatantId,
    grid: &G,
    scale: CellScale,
    from: Idx,
    movement: &Movement<F>,
) -> Vec<(Idx, Cost)>
where
    G: Grid,
    F: Fn(Step<G::Cell>) -> Option<Cost>,
{
    let allowance = match combat.combatant(who) {
        Some(c) => scale.cells(c.encounter_rate()),
        None => return Vec::new(),
    };
    grid.reachable(from, allowance, movement)
}
