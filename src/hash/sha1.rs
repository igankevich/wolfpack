use crate::hash::HashArray;
use crate::hash::Hasher;
use crate::hash::HashingReader;

impl Hasher for Sha1 {
    type Output = Sha1Hash;

    fn new() -> Self {
        sha2::Digest::new()
    }

    fn update(&mut self, data: &[u8]) {
        sha2::Digest::update(self, data);
    }

    fn finalize(self) -> Self::Output {
        Sha1Hash::new(sha2::Digest::finalize(self).into())
    }
}

pub type Sha1 = sha1::Sha1;
pub type Sha1Hash = HashArray<20>;
pub type Sha1Reader<R> = HashingReader<R, Sha1>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::tests::*;

    #[test]
    fn sha1() {
        same_as_computing_hash_of_the_whole_file::<Sha1>();
        display_parse::<Sha1Hash>();
    }
}
