//! Dice and the single source of randomness.
//!
//! All randomness enters the crate through [`DiceRoller`]. Supply your own
//! implementation, or use the built-in seeded [`SeededDice`]. A fixed seed
//! replays a combat exactly.

/// A die, named by its number of faces.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Die {
    /// Two faces.
    D2,
    /// Three faces.
    D3,
    /// Four faces.
    D4,
    /// Six faces.
    D6,
    /// Eight faces.
    D8,
    /// Ten faces.
    D10,
    /// Twelve faces.
    D12,
    /// Twenty faces.
    D20,
    /// One hundred faces.
    D100,
}

impl Die {
    /// The number of faces.
    pub const fn faces(self) -> u8 {
        match self {
            Die::D2 => 2,
            Die::D3 => 3,
            Die::D4 => 4,
            Die::D6 => 6,
            Die::D8 => 8,
            Die::D10 => 10,
            Die::D12 => 12,
            Die::D20 => 20,
            Die::D100 => 100,
        }
    }
}

/// The source of randomness.
///
/// The trait is dyn-compatible so hooks and custom effects can take
/// `&mut dyn DiceRoller`.
pub trait DiceRoller {
    /// Return a uniform roll in `1..=die.faces()`.
    fn roll(&mut self, die: Die) -> u8;
}

/// A dice expression such as `2d6+1`, as data.
///
/// A `count` of zero is legal and rolls to `bonus`.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DiceExpr {
    /// How many dice to roll.
    pub count: u8,
    /// The die to roll.
    pub die: Die,
    /// A flat bonus or penalty added to the sum.
    pub bonus: i8,
}

impl DiceExpr {
    /// A flat value with no dice.
    pub const fn flat(value: i8) -> Self {
        DiceExpr {
            count: 0,
            die: Die::D6,
            bonus: value,
        }
    }

    /// `count` dice of `die`, no bonus.
    pub const fn of(count: u8, die: Die) -> Self {
        DiceExpr {
            count,
            die,
            bonus: 0,
        }
    }

    /// Add a flat bonus (or penalty) to this expression.
    #[must_use]
    pub const fn plus(self, bonus: i8) -> Self {
        DiceExpr {
            bonus: self.bonus.saturating_add(bonus),
            ..self
        }
    }

    /// Roll the expression.
    pub fn roll(self, r: &mut dyn DiceRoller) -> i16 {
        let mut sum = i16::from(self.bonus);
        for _ in 0..self.count {
            sum += i16::from(r.roll(self.die));
        }
        sum
    }
}

/// A seeded pseudo-random [`DiceRoller`] (xoshiro256**).
///
/// `SeededDice` is `Clone` and `Eq`: snapshot the roller with the combat and
/// a replay from the same state produces the same rolls. It is not
/// cryptographic.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SeededDice {
    s: [u64; 4],
}

impl SeededDice {
    /// Create a roller from a seed. Any seed is valid.
    pub fn seeded(seed: u64) -> Self {
        // splitmix64 expands the seed so a small seed still fills the state.
        let mut x = seed;
        let mut next = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        SeededDice {
            s: [next(), next(), next(), next()],
        }
    }

    fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }
}

impl DiceRoller for SeededDice {
    fn roll(&mut self, die: Die) -> u8 {
        let faces = u64::from(die.faces());
        // Rejection sampling keeps the roll unbiased.
        let zone = u64::MAX - u64::MAX % faces;
        loop {
            let v = self.next_u64();
            if v < zone {
                return (v % faces) as u8 + 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_stay_in_range() {
        let mut r = SeededDice::seeded(1);
        for die in [
            Die::D2,
            Die::D3,
            Die::D4,
            Die::D6,
            Die::D8,
            Die::D10,
            Die::D12,
            Die::D20,
            Die::D100,
        ] {
            for _ in 0..1000 {
                let v = r.roll(die);
                assert!((1..=die.faces()).contains(&v));
            }
        }
    }

    #[test]
    fn same_seed_same_sequence() {
        let mut a = SeededDice::seeded(48317);
        let mut b = SeededDice::seeded(48317);
        for _ in 0..100 {
            assert_eq!(a.roll(Die::D20), b.roll(Die::D20));
        }
    }

    #[test]
    fn every_face_appears() {
        let mut r = SeededDice::seeded(7);
        let mut seen = [false; 20];
        for _ in 0..2000 {
            seen[usize::from(r.roll(Die::D20)) - 1] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn expr_rolls_and_flat() {
        let mut r = SeededDice::seeded(3);
        let v = DiceExpr::of(2, Die::D6).plus(1).roll(&mut r);
        assert!((3..=13).contains(&v));
        assert_eq!(DiceExpr::flat(-2).roll(&mut r), -2);
    }
}
