use std::io::Error;
use std::io::Write;
use std::path::PathBuf;

use crate::sign::Signer;

pub struct SignatureWriter<S: Signer, W: Write> {
    writer: W,
    signer: S,
    signature_file: PathBuf,
    buffer: Vec<u8>,
}

impl<S: Signer, W: Write> SignatureWriter<S, W> {
    pub fn new(writer: W, signer: S, signature_file: PathBuf) -> Self {
        Self {
            writer,
            signer,
            signature_file,
            buffer: Default::default(),
        }
    }

    pub fn write_signature(self) -> Result<(), Error> {
        self.do_write_signature()
    }

    fn do_write_signature(&self) -> Result<(), Error> {
        let signature = self
            .signer
            .sign(&self.buffer[..])
            .map_err(|_| std::io::Error::other("failed to sign"))?;
        std::fs::write(self.signature_file.as_path(), signature)
    }
}

impl<S: Signer, W: Write> Write for SignatureWriter<S, W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let n = self.writer.write(buf)?;
        self.buffer.extend_from_slice(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> Result<(), Error> {
        self.writer.flush()
    }
}

impl<S: Signer, W: Write> Drop for SignatureWriter<S, W> {
    fn drop(&mut self) {
        let _ = self.do_write_signature();
    }
}
