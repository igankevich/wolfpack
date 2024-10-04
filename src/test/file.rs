use std::fs::create_dir_all;
use std::path::Path;

use arbitrary::Arbitrary;
use arbitrary::Unstructured;
use rand::Rng;
use rand_mt::Mt64;
use tempfile::TempDir;

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
        let chars = valid_path_chars();
        let dir = TempDir::new().unwrap();
        let num_files: usize = rng.gen_range(1..=10);
        for _ in 0..num_files {
            let num_comp: usize = rng.gen_range(1..=10);
            let mut path = dir.path().to_path_buf();
            for _ in 0..num_comp {
                let comp = loop {
                    let num_chars = rng.gen_range(1..=10);
                    let mut comp = String::with_capacity(num_chars);
                    for _ in 0..num_chars {
                        comp.push(chars[rng.gen_range(0..chars.len())] as char);
                    }
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
            eprintln!("path {:?}", path);
            create_dir_all(path.parent().unwrap()).unwrap();
            let mut contents: ByteArray = [0; 4096];
            rng.fill_bytes(&mut contents);
            std::fs::write(path, &contents[..]).unwrap();
        }
        Ok(Self { dir })
    }
}

fn valid_path_chars() -> Vec<u8> {
    disjoint_intervals([1, b'/', u8::MAX])
}

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
