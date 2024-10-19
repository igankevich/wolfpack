use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Debug;
use std::ops::Deref;
use std::str::FromStr;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct HashArray<const N: usize>([u8; N]);

impl<const N: usize> HashArray<N> {
    pub fn new(array: [u8; N]) -> Self {
        Self(array)
    }
}

impl<const N: usize> From<[u8; N]> for HashArray<N> {
    fn from(data: [u8; N]) -> Self {
        Self(data)
    }
}

impl<const N: usize> TryFrom<&[u8]> for HashArray<N> {
    type Error = HashTryFromError;
    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self(data.try_into().map_err(|_| HashTryFromError)?))
    }
}

impl<const N: usize> From<HashArray<N>> for [u8; N] {
    fn from(hash: HashArray<N>) -> Self {
        hash.0
    }
}

impl<const N: usize> Deref for HashArray<N> {
    type Target = [u8; N];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> Display for HashArray<N> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for x in self.0.iter() {
            write!(f, "{:02x}", x)?;
        }
        Ok(())
    }
}

impl<const N: usize> Debug for HashArray<N> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl<const N: usize> FromStr for HashArray<N> {
    type Err = HashParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut array = [0_u8; N];
        let n = s.len();
        if n != 2 * N {
            return Err(HashParseError);
        }
        for (i, byte) in array.iter_mut().enumerate() {
            let j = 2 * i;
            *byte = u8::from_str_radix(&s[j..=(j + 1)], 16).map_err(|_| HashParseError)?;
        }
        Ok(Self(array))
    }
}

#[derive(Debug)]
pub struct HashParseError;

#[derive(Debug)]
pub struct HashTryFromError;
