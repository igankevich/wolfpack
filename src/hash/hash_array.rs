use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Deref;

#[derive(PartialEq, Eq, Debug)]
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
