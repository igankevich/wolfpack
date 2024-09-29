use std::io::Read;

use sha2::Sha256;
use sha2::Digest;
use crate::deb::Sha2Digest;
use crate::deb::Hash;

pub struct Sha256Reader<R: Read> {
    reader: R,
    sha2: Sha256,
    nread: usize,
}

impl<R: Read> Sha256Reader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            sha2: Sha256::new(),
            nread: 0,
        }
    }

    pub fn consume(&mut self) -> Result<(), std::io::Error> {
        let mut buf = [0_u8; 4096];
        while self.read(&mut buf[..])? != 0 {}
        Ok(())
    }

    pub fn digest(mut self) -> Result<(Sha2Digest, usize), std::io::Error> {
        self.consume()?;
        Ok((
            Hash::<32>(self.sha2.finalize().into()),
            self.nread,
        ))
    }
}

impl<R: Read> Read for Sha256Reader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let n = self.reader.read(buf)?;
        self.nread += n;
        self.sha2.update(&buf[..n]);
        Ok(n)
    }
}
