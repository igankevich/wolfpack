use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::Deref;
use std::str::FromStr;

use base16ct::lower::encode_string;
use base16ct::mixed::decode;
use base64ct::Base64;
use base64ct::Encoding;
use constant_time_eq::constant_time_eq_n;

#[derive(PartialOrd, Ord, Clone)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct HashArray<const N: usize>([u8; N]);

impl<const N: usize> HashArray<N> {
    pub const fn new(array: [u8; N]) -> Self {
        Self(array)
    }

    pub const fn len(&self) -> usize {
        N
    }

    pub const fn is_empty(&self) -> bool {
        N == 0
    }

    pub fn to_base64(&self) -> String {
        Base64::encode_string(&self[..])
    }

    pub const LEN: usize = N;
    pub const HEX_LEN: usize = 2 * N;
}

impl<const N: usize> PartialEq for HashArray<N> {
    fn eq(&self, other: &Self) -> bool {
        constant_time_eq_n(&self.0, &other.0)
    }
}

impl<const N: usize> Eq for HashArray<N> {}

impl<const N: usize> Hash for HashArray<N> {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.0.hash(state);
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
        let s = encode_string(&self[..]);
        f.write_str(&s)
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
        decode(s.as_bytes(), &mut array[..]).map_err(|_| HashParseError)?;
        Ok(Self(array))
    }
}

#[derive(Debug)]
pub struct HashParseError;

#[derive(Debug)]
pub struct HashTryFromError;
