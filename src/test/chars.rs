use std::ops::RangeInclusive;

use arbitrary::Unstructured;
use gcollections::ops::*;
use interval::ops::*;
use interval::Interval;
use interval::IntervalSet;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct Chars {
    intervals: IntervalSet<u32>,
}

impl Chars {
    pub fn union<O: Into<Self>>(&self, other: O) -> Self {
        let other: Self = other.into();
        Self {
            intervals: self.intervals.union(&other.intervals),
        }
    }

    pub fn difference<O: Into<Self>>(&self, other: O) -> Self {
        let other: Self = other.into();
        Self {
            intervals: self.intervals.difference(&other.intervals),
        }
    }

    pub fn get(&self, mut i: u32) -> Option<char> {
        for interval in self.intervals.iter() {
            let s = interval.size();
            if i < s {
                return Some(
                    char::from_u32(interval.lower() + i)
                        .expect(&format!("failed on {:x}", interval.lower() + i)),
                );
            }
            i -= s;
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.intervals.is_empty()
    }

    pub fn len(&self) -> u32 {
        self.intervals.size()
    }

    pub fn contains(&self, ch: char) -> bool {
        let ch = ch as u32;
        self.intervals.contains(&ch)
    }

    pub fn arbitrary_char(&self, u: &mut Unstructured<'_>) -> arbitrary::Result<char> {
        let i = u.int_in_range(0..=(self.len() - 1))?;
        Ok(self.get(i).expect("should not fail"))
    }

    pub fn arbitrary_string(
        &self,
        u: &mut Unstructured<'_>,
        len: usize,
    ) -> arbitrary::Result<String> {
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            s.push(self.arbitrary_char(u)?);
        }
        Ok(s)
    }

    pub fn random_char<R: Rng>(&self, rng: &mut R) -> char {
        let i = rng.gen_range(0..=(self.len() - 1));
        self.get(i).expect("should not fail")
    }

    pub fn random_string<R: Rng>(&self, rng: &mut R, len: usize) -> String {
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            s.push(self.random_char(rng));
        }
        s
    }
}

impl From<RangeInclusive<char>> for Chars {
    fn from(other: RangeInclusive<char>) -> Self {
        let (a, b) = other.into_inner();
        let mut intervals = IntervalSet::empty();
        intervals.extend([Interval::new(a as u32, b as u32)]);
        Self { intervals }
    }
}

impl<const N: usize> From<[RangeInclusive<char>; N]> for Chars {
    fn from(other: [RangeInclusive<char>; N]) -> Self {
        let mut iter = other.into_iter();
        let mut set = Self::from(iter.next().unwrap());
        for range in iter {
            set = set.union(range);
        }
        set
    }
}

impl From<&[char]> for Chars {
    fn from(other: &[char]) -> Self {
        let mut intervals = IntervalSet::empty();
        for ch in other {
            intervals.extend([Interval::new(*ch as u32, *ch as u32)]);
        }
        Self { intervals }
    }
}

impl<const N: usize> From<[char; N]> for Chars {
    fn from(other: [char; N]) -> Self {
        Self::from(&other[..])
    }
}
