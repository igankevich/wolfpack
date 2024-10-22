use std::fs::create_dir_all;
use std::path::Path;

use arbitrary::Arbitrary;
use arbitrary::Unstructured;
use rand::Rng;
use rand_mt::Mt64;
use tempfile::TempDir;

use crate::test::Chars;
use crate::test::CONTROL;
//use crate::test::UNICODE;
use crate::test::ASCII_LOWERCASE;

pub struct DirectoryOfFiles {
    #[allow(dead_code)]
    dir: TempDir,
}

impl DirectoryOfFiles {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

impl<'a> Arbitrary<'a> for DirectoryOfFiles {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        type ByteArray = [u8; 4096];
        let seed: u64 = u.arbitrary()?;
        let mut rng = Mt64::new(seed);
        let chars = Chars::from(ASCII_LOWERCASE)
            .difference(CONTROL)
            .difference(['/']);
        //let chars = valid_path_chars();
        let dir = TempDir::new().unwrap();
        let num_files: usize = rng.gen_range(1..=10);
        for _ in 0..num_files {
            let num_comp: usize = rng.gen_range(1..=10);
            let mut path = dir.path().to_path_buf();
            for _ in 0..num_comp {
                let comp = loop {
                    let num_chars = rng.gen_range(1..=10);
                    let comp = chars.random_string(&mut rng, num_chars);
                    if [".", ".."].contains(&comp.as_str()) {
                        continue;
                    }
                    break comp;
                };
                path.push(comp);
                if path.as_os_str().len() > 100 {
                    path.pop();
                }
            }
            create_dir_all(path.parent().unwrap()).unwrap();
            let mut contents: ByteArray = [0; 4096];
            rng.fill_bytes(&mut contents);
            std::fs::write(path, &contents[..]).unwrap();
        }
        Ok(Self { dir })
    }
}

// TODO need char version of that without generating a Vec
pub fn disjoint_intervals<I: IntoIterator<Item = u8>>(breakpoints: I) -> Vec<u8> {
    let mut values = Vec::new();
    let mut iter = breakpoints.into_iter();
    let mut start = match iter.next() {
        Some(value) => value,
        None => return values,
    };
    for end in iter {
        values.extend(start..end);
        start = end.saturating_add(1);
    }
    values
}

pub const MS_DOS_NEWLINE: char = '\x1a';
