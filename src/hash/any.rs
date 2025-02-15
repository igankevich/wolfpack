use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

use crate::hash::HashParseError;
use crate::hash::HashTryFromError;
use crate::hash::Hasher;
use crate::hash::Md5Hash;
use crate::hash::Md5Hasher;
use crate::hash::Sha1Hash;
use crate::hash::Sha1Hasher;
use crate::hash::Sha256Hash;
use crate::hash::Sha256Hasher;
use crate::hash::Sha512Hash;
use crate::hash::Sha512Hasher;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum AnyHash {
    Md5(Md5Hash),
    Sha1(Sha1Hash),
    Sha256(Sha256Hash),
    Sha512(Sha512Hash),
}

#[allow(clippy::len_without_is_empty)]
impl AnyHash {
    pub fn hasher(&self) -> AnyHasher {
        use AnyHash::*;
        match self {
            Md5(..) => AnyHasher::Md5(Md5Hasher::new()),
            Sha1(..) => AnyHasher::Sha1(Sha1Hasher::new()),
            Sha256(..) => AnyHasher::Sha256(Sha256Hasher::new()),
            Sha512(..) => AnyHasher::Sha512(Sha512Hasher::new()),
        }
    }

    pub const fn len(&self) -> usize {
        use AnyHash::*;
        match self {
            Md5(..) => Md5Hash::LEN,
            Sha1(..) => Sha1Hash::LEN,
            Sha256(..) => Sha256Hash::LEN,
            Sha512(..) => Sha512Hash::LEN,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        use AnyHash::*;
        match self {
            Md5(h) => h.as_ref(),
            Sha1(h) => h.as_ref(),
            Sha256(h) => h.as_ref(),
            Sha512(h) => h.as_ref(),
        }
    }
}

impl Display for AnyHash {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        use AnyHash::*;
        match self {
            Md5(h) => Display::fmt(h, f),
            Sha1(h) => Display::fmt(h, f),
            Sha256(h) => Display::fmt(h, f),
            Sha512(h) => Display::fmt(h, f),
        }
    }
}

impl FromStr for AnyHash {
    type Err = HashParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.len() {
            Md5Hash::HEX_LEN => Ok(Self::Md5(s.parse()?)),
            Sha1Hash::HEX_LEN => Ok(Self::Sha1(s.parse()?)),
            Sha256Hash::HEX_LEN => Ok(Self::Sha256(s.parse()?)),
            Sha512Hash::HEX_LEN => Ok(Self::Sha512(s.parse()?)),
            _ => Err(HashParseError),
        }
    }
}

impl TryFrom<&[u8]> for AnyHash {
    type Error = HashTryFromError;
    fn try_from(s: &[u8]) -> Result<Self, Self::Error> {
        match s.len() {
            Md5Hash::LEN => Ok(Self::Md5(s.try_into()?)),
            Sha1Hash::LEN => Ok(Self::Sha1(s.try_into()?)),
            Sha256Hash::LEN => Ok(Self::Sha256(s.try_into()?)),
            Sha512Hash::LEN => Ok(Self::Sha512(s.try_into()?)),
            _ => Err(HashTryFromError),
        }
    }
}

impl_from!(Md5Hash, Md5);
impl_from!(Sha1Hash, Sha1);
impl_from!(Sha256Hash, Sha256);
impl_from!(Sha512Hash, Sha512);

macro_rules! impl_from {
    ($from:ty, $self:ident) => {
        impl From<$from> for AnyHash {
            fn from(other: $from) -> Self {
                Self::$self(other)
            }
        }
    };
}

use impl_from;

pub enum AnyHasher {
    Md5(Md5Hasher),
    Sha1(Sha1Hasher),
    Sha256(Sha256Hasher),
    Sha512(Sha512Hasher),
}

impl Hasher for AnyHasher {
    type Output = AnyHash;

    fn new() -> Self {
        Self::Sha256(Sha256Hasher::new())
    }

    fn update(&mut self, data: &[u8]) {
        use AnyHasher::*;
        match self {
            Md5(hasher) => hasher.update(data),
            Sha1(hasher) => hasher.update(data),
            Sha256(hasher) => hasher.update(data),
            Sha512(hasher) => hasher.update(data),
        }
    }

    fn finalize(self) -> Self::Output {
        use AnyHasher::*;
        match self {
            Md5(hasher) => hasher.finalize().into(),
            Sha1(hasher) => hasher.finalize().into(),
            Sha256(hasher) => hasher.finalize().into(),
            Sha512(hasher) => hasher.finalize().into(),
        }
    }
}
