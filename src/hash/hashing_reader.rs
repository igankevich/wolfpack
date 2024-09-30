use std::io::Read;

use crate::hash::Hasher;

pub struct HashingReader<R: Read, H: Hasher> {
    reader: R,
    hasher: H,
    nread: usize,
}

impl<R: Read, H: Hasher> HashingReader<R, H> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            hasher: H::new(),
            nread: 0,
        }
    }

    pub fn consume(&mut self) -> Result<(), std::io::Error> {
        let mut buf = [0_u8; BUFFER_LEN];
        while self.read(&mut buf[..])? != 0 {}
        Ok(())
    }

    pub fn digest(mut self) -> Result<(H::Output, usize), std::io::Error> {
        self.consume()?;
        Ok((self.hasher.finalize(), self.nread))
    }
}

impl<R: Read, H: Hasher> Read for HashingReader<R, H> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let n = self.reader.read(buf)?;
        self.nread += n;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

const BUFFER_LEN: usize = 4096;

#[cfg(test)]
pub mod tests {

    use arbtest::arbtest;
    use rand::rngs::OsRng;
    use rand::RngCore;
    use std::fmt::Debug;

    use super::*;

    pub fn same_as_computing_hash_of_the_whole_file<H: Hasher>()
    where
        H::Output: PartialEq + Debug,
    {
        arbtest(|u| {
            let spread = BUFFER_LEN / 2;
            let mut data =
                vec![0_u8; BUFFER_LEN - spread + u.int_in_range::<usize>(0..=2 * spread)?];
            OsRng.fill_bytes(&mut data);
            let (actual_hash, size) = HashingReader::<&[u8], H>::new(&data[..]).digest().unwrap();
            assert_eq!(data.len(), size);
            let mut hasher = H::new();
            hasher.update(&data);
            let expected_hash = hasher.finalize();
            assert_eq!(expected_hash, actual_hash);
            Ok(())
        });
    }
}
