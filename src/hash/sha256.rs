use sha2::Sha256;

use crate::hash::HashArray;
use crate::hash::Hasher;
use crate::hash::HashingReader;

impl Hasher for Sha256 {
    type Output = Sha256Hash;

    fn new() -> Self {
        sha2::Digest::new()
    }

    fn update(&mut self, data: &[u8]) {
        sha2::Digest::update(self, data);
    }

    fn finalize(self) -> Self::Output {
        Sha256Hash::new(sha2::Digest::finalize(self).into())
    }
}

pub type Sha256Hash = HashArray<32>;
pub type Sha256Reader<R> = HashingReader<R, Sha256>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::tests::*;

    #[test]
    fn sha256() {
        same_as_computing_hash_of_the_whole_file::<Sha256>();
        display_parse::<Sha256Hash>();
    }
}
