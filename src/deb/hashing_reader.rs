use std::fmt::Display;
use std::fmt::Formatter;
use std::io::Read;

use sha1::Digest;
use sha1::Sha1;
use sha2::Sha256;

pub struct HashingReader<R: Read> {
    reader: R,
    md5: md5::Context,
    sha1: Sha1,
    sha2: Sha256,
    nread: usize,
}

impl<R: Read> HashingReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            md5: md5::Context::new(),
            sha1: Sha1::new(),
            sha2: Sha256::new(),
            nread: 0,
        }
    }

    pub fn consume(&mut self) -> Result<(), std::io::Error> {
        let mut buf = [0_u8; 4096];
        while self.read(&mut buf[..])? != 0 {}
        Ok(())
    }

    pub fn digest(mut self) -> Result<(Md5Digest, Sha1Digest, Sha2Digest, usize), std::io::Error> {
        self.consume()?;
        Ok((
            self.md5.compute(),
            Hash::<20>(self.sha1.finalize().into()),
            Hash::<32>(self.sha2.finalize().into()),
            self.nread,
        ))
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let n = self.reader.read(buf)?;
        self.nread += n;
        self.md5.consume(&buf[..n]);
        self.sha1.update(&buf[..n]);
        self.sha2.update(&buf[..n]);
        Ok(n)
    }
}

pub type Md5Digest = md5::Digest;
pub type Sha1Digest = Hash<20>;
pub type Sha2Digest = Hash<32>;

pub struct Hash<const N: usize>(pub [u8; N]);

impl<const N: usize> From<[u8; N]> for Hash<N> {
    fn from(data: [u8; N]) -> Self {
        Self(data)
    }
}

impl<const N: usize> From<Hash<N>> for [u8; N] {
    fn from(hash: Hash<N>) -> Self {
        hash.0
    }
}

impl<const N: usize> Display for Hash<N> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for x in self.0.iter() {
            write!(f, "{:02x}", x)?;
        }
        Ok(())
    }
}
