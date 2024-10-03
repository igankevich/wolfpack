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

pub(crate) const BUFFER_LEN: usize = 4096;
