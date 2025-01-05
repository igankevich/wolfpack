use crate::hash::HashArray;
use crate::hash::Hasher;
use crate::hash::HashingReader;

impl Hasher for md5::Context {
    type Output = Md5Hash;

    fn new() -> Self {
        md5::Context::new()
    }

    fn update(&mut self, data: &[u8]) {
        self.consume(data);
    }

    fn finalize(self) -> Self::Output {
        Md5Hash::new(self.compute().into())
    }
}

pub type Md5Hash = HashArray<16>;
pub type Md5Reader<R> = HashingReader<R, md5::Context>;
pub type Md5Hasher = md5::Context;

impl From<md5::Digest> for Md5Hash {
    fn from(other: md5::Digest) -> Self {
        other.0.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::tests::*;

    #[test]
    fn md5() {
        same_as_computing_hash_of_the_whole_file::<md5::Context>();
        display_parse::<Md5Hash>();
    }
}
