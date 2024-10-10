use std::io::Read;
use std::path::PathBuf;

use crate::sign::Verifier;

pub struct VerifyingReader<R: Read, V: Verifier> {
    reader: R,
    verifier: V,
    signature_file: PathBuf,
    buffer: Vec<u8>,
    nread: usize,
    verified: bool,
}

impl<R: Read, V: Verifier> VerifyingReader<R, V> {
    pub fn new(reader: R, verifier: V, signature_file: PathBuf) -> Self {
        Self {
            reader,
            verifier,
            signature_file,
            buffer: Default::default(),
            nread: 0,
            verified: false,
        }
    }
}

impl<R: Read, V: Verifier> Read for VerifyingReader<R, V> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if !self.verified {
            self.reader.read_to_end(&mut self.buffer)?;
            let signature = std::fs::read(self.signature_file.as_path())?;
            self.verifier
                .verify(&self.buffer[..], &signature[..])
                .map_err(|_| std::io::Error::other("signature verification failed"))?;
            self.verified = true;
        }
        let n = Read::read(&mut &self.buffer[self.nread..], buf)?;
        self.nread += n;
        Ok(n)
    }
}
