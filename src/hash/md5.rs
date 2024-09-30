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
        self.compute()
    }
}

pub type Md5Hash = md5::Digest;
pub type Md5Reader<R> = HashingReader<R, md5::Context>;

#[cfg(test)]
mod tests {
    use crate::hash::tests::*;

    #[test]
    fn md5() {
        same_as_computing_hash_of_the_whole_file::<md5::Context>();
    }
}
