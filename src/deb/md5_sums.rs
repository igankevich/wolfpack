use std::fmt::Write;
use std::path::Path;

use crate::hash::Md5Hash;

pub struct Md5Sums {
    contents: String,
}

impl Md5Sums {
    pub fn new() -> Self {
        Self {
            contents: String::with_capacity(4096),
        }
    }

    pub fn append_file(&mut self, path: &Path, hash: Md5Hash) {
        let _ = writeln!(&mut self.contents, "{:x}  {}", hash, path.display());
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.contents.as_bytes()
    }
}

impl Default for Md5Sums {
    fn default() -> Self {
        Self::new()
    }
}
