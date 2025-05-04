use crate::hash::Hasher;
use crate::hash::HashingReader;
use crate::hash::Sha1;
use crate::hash::Sha1Hash;
use crate::hash::Sha256;
use crate::hash::Sha256Hash;
use crate::hash::Sha512;
use crate::hash::Sha512Hash;

pub struct MultiHasher {
    md5: md5::Context,
    sha1: Sha1,
    sha256: Sha256,
    sha512: Sha512,
}

#[derive(PartialEq, Eq, Debug)]
pub struct MultiHash {
    pub md5: md5::Digest,
    pub sha1: Sha1Hash,
    pub sha256: Sha256Hash,
    pub sha512: Sha512Hash,
}

impl Hasher for MultiHasher {
    type Output = MultiHash;

    fn new() -> Self {
        Self {
            md5: md5::Context::new(),
            sha1: sha1::Digest::new(),
            sha256: sha2::Digest::new(),
            sha512: sha2::Digest::new(),
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.md5.consume(data);
        sha1::Digest::update(&mut self.sha1, data);
        sha2::Digest::update(&mut self.sha256, data);
        sha2::Digest::update(&mut self.sha512, data);
    }

    fn finalize(self) -> Self::Output {
        MultiHash {
            md5: self.md5.compute(),
            sha1: Sha1Hash::new(sha1::Digest::finalize(self.sha1).into()),
            sha256: Sha256Hash::new(sha2::Digest::finalize(self.sha256).into()),
            sha512: Sha512Hash::new(sha2::Digest::finalize(self.sha512).into()),
        }
    }
}

pub type MultiHashReader<R> = HashingReader<R, MultiHasher>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::tests::*;

    #[test]
    fn multi_hash() {
        same_as_computing_hash_of_the_whole_file::<MultiHasher>();
        display_parse::<Sha1Hash>();
    }
}
