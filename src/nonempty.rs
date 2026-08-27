//! A collection that always holds at least one element.

use alloc::vec::Vec;
use core::num::NonZeroUsize;

/// A list with at least one element, by construction.
///
/// Use it where emptiness would make the value nonsense: a side's roster, a
/// monster's attack routine, a weapon's attack modes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NonEmpty<T> {
    head: T,
    tail: Vec<T>,
}

impl<T> NonEmpty<T> {
    /// A list of one element.
    pub const fn of(head: T) -> Self {
        NonEmpty {
            head,
            tail: Vec::new(),
        }
    }

    /// Convert from a `Vec`. Returns `None` when the vector is empty.
    pub fn from_vec(mut v: Vec<T>) -> Option<Self> {
        if v.is_empty() {
            return None;
        }
        let head = v.remove(0);
        Some(NonEmpty { head, tail: v })
    }

    /// The first element.
    pub const fn first(&self) -> &T {
        &self.head
    }

    /// The number of elements. Never zero.
    pub fn len(&self) -> NonZeroUsize {
        NonZeroUsize::new(1 + self.tail.len()).expect("1 + len is never zero")
    }

    /// The element at `index`, if present.
    pub fn get(&self, index: usize) -> Option<&T> {
        if index == 0 {
            Some(&self.head)
        } else {
            self.tail.get(index - 1)
        }
    }

    /// Append an element.
    pub fn push(&mut self, value: T) {
        self.tail.push(value);
    }

    /// Iterate over the elements in order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        core::iter::once(&self.head).chain(self.tail.iter())
    }
}

impl<'a, T> IntoIterator for &'a NonEmpty<T> {
    type Item = &'a T;
    type IntoIter = core::iter::Chain<core::iter::Once<&'a T>, core::slice::Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(&self.head).chain(self.tail.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn empty_vec_is_rejected() {
        assert!(NonEmpty::<u8>::from_vec(vec![]).is_none());
    }

    #[test]
    fn order_is_kept() {
        let mut n = NonEmpty::of(1);
        n.push(2);
        n.push(3);
        assert_eq!(n.len().get(), 3);
        assert_eq!(n.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(n.get(2), Some(&3));
        assert_eq!(n.get(3), None);
    }
}
