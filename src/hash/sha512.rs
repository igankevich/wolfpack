use crate::hash::HashArray;
use crate::hash::Hasher;
use crate::hash::HashingReader;

impl Hasher for Sha512 {
    type Output = Sha512Hash;

    fn new() -> Self {
        sha2::Digest::new()
    }

    fn update(&mut self, data: &[u8]) {
        sha2::Digest::update(self, data);
    }

    fn finalize(self) -> Self::Output {
        Sha512Hash::new(sha2::Digest::finalize(self).into())
    }
}

pub type Sha512 = sha2::Sha512;
pub type Sha512Hash = HashArray<64>;
pub type Sha512Reader<R> = HashingReader<R, Sha512>;
pub type Sha512Hasher = Sha512;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::tests::*;

    #[test]
    fn sha512() {
        same_as_computing_hash_of_the_whole_file::<Sha512>();
        display_parse::<Sha512Hash>();
    }
}
