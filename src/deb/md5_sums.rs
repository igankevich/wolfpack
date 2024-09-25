use std::fmt::Write;
use std::io::Read;
use std::path::Path;

pub struct Md5Sums {
    contents: String,
}

impl Md5Sums {
    pub fn new() -> Self {
        Self {
            contents: String::with_capacity(4096),
        }
    }

    pub fn append_file(&mut self, path: &Path, digest: md5::Digest) {
        let _ = writeln!(&mut self.contents, "{:x}  {}", digest, path.display());
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.contents.as_bytes()
    }
}

pub struct Md5Reader<R: Read> {
    reader: R,
    context: md5::Context,
}

impl<R: Read> Md5Reader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            context: md5::Context::new(),
        }
    }

    pub fn digest(self) -> md5::Digest {
        self.context.compute()
    }
}

impl<R: Read> Read for Md5Reader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let n = self.reader.read(buf)?;
        self.context.consume(&buf[..n]);
        Ok(n)
    }
}
