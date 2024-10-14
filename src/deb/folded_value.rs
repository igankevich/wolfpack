use std::cmp::Ordering;
use std::fmt::Display;
use std::fmt::Formatter;
use std::hash::Hash;
use std::hash::Hasher;

use crate::deb::Error;
use crate::deb::SimpleValue;

#[derive(Clone, Debug)]
pub struct FoldedValue(String);

impl FoldedValue {
    pub fn new(value: &str) -> Self {
        let mut buf = String::with_capacity(value.len());
        let mut words = value.split_whitespace();
        if let Some(word) = words.next() {
            buf.push_str(word);
        }
        for word in words {
            buf.push(' ');
            buf.push_str(word);
        }
        Self(buf)
    }

    pub fn try_from(value: String) -> Result<Self, Error> {
        if value.is_empty() {
            return Err(Error::FieldValue(format!("empty {value}")));
        }
        if value.starts_with(char::is_whitespace) {
            return Err(Error::FieldValue(format!("whitespace {value}")));
        }
        if value
            .split('\n')
            .skip(1)
            .any(|line| line.is_empty() || line == "." || line.chars().all(char::is_whitespace))
        {
            return Err(Error::FieldValue(format!("empty line {value}")));
        }
        Ok(Self(value))
    }

    pub(crate) fn words(&self) -> impl Iterator<Item = &str> {
        self.0.split_whitespace()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl PartialEq for FoldedValue {
    fn eq(&self, other: &Self) -> bool {
        self.words().eq(other.words())
    }
}

impl Eq for FoldedValue {}

impl PartialOrd for FoldedValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FoldedValue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.words().cmp(other.words())
    }
}

impl Hash for FoldedValue {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let mut words = self.words();
        if let Some(word) = words.next() {
            word.hash(state);
        }
        for word in words {
            ' '.hash(state);
            word.hash(state);
        }
    }
}

impl Display for FoldedValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let mut lines = self
            .0
            .split(&['\r', '\n'])
            .filter(|line| !line.is_empty() && !line.chars().all(|ch| [' ', '\t'].contains(&ch)));
        if let Some(line) = lines.next() {
            write!(f, "{}", line)?;
        }
        for line in lines {
            write!(f, "\n {}", line)?;
        }
        Ok(())
    }
}

impl TryFrom<String> for FoldedValue {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&str> for FoldedValue {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut folded = String::with_capacity(value.len());
        let mut lines = value.split('\n');
        // parse the first line verbatim
        if let Some(line) = lines.next() {
            folded.push_str(line);
            folded.push('\n');
        }
        for line in lines {
            if line.starts_with([' ', '\t']) {
                folded.push_str(&line[1..]);
                folded.push('\n');
            } else {
                folded.push_str(line);
                folded.push('\n');
            }
        }
        if !folded.is_empty() {
            folded.pop();
        }
        Self::try_from(folded)
    }
}

impl From<FoldedValue> for String {
    fn from(v: FoldedValue) -> Self {
        v.0
    }
}

impl From<SimpleValue> for FoldedValue {
    fn from(other: SimpleValue) -> Self {
        Self(other.into())
    }
}

impl PartialEq<SimpleValue> for FoldedValue {
    fn eq(&self, other: &SimpleValue) -> bool {
        self.as_str().eq(other.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;

    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;
    use rand::Rng;
    use rand_mt::Mt64;

    use super::*;
    use crate::deb::SimpleValue;
    use crate::test::disjoint_intervals;

    #[test]
    fn folded_value_whitespace_is_insignificant() {
        arbtest(|u| {
            let s1: String = u.arbitrary()?;
            let s2 = s1.replace(char::is_whitespace, "  ");
            let value1 = FoldedValue(s1);
            let value2 = FoldedValue(s2);
            assert_eq!(value1, value2);
            Ok(())
        });
    }

    #[test]
    fn folded_to_simple() {
        arbtest(|u| {
            let s: String = u.arbitrary()?;
            let expected = FoldedValue(s);
            let simple: SimpleValue = expected.clone().into();
            let actual: FoldedValue = simple.clone().into();
            assert_eq!(expected, actual, "simple = {:?}", simple);
            Ok(())
        });
    }

    #[test]
    fn folded_display_parse() {
        arbtest(|u| {
            let expected: FoldedValue = u.arbitrary()?;
            let string = expected.to_string();
            let actual = FoldedValue::try_from(string.clone()).unwrap();
            assert_eq!(expected, actual, "string = {:?}", string);
            assert_eq!(
                expected.cmp(&actual),
                Ordering::Equal,
                "string = {:?}",
                string
            );
            assert_eq!(
                actual.cmp(&expected),
                Ordering::Equal,
                "string = {:?}",
                string
            );
            let hash = {
                let mut hasher = DefaultHasher::new();
                expected.hash(&mut hasher);
                hasher.finish()
            };
            let actual_hash = {
                let mut hasher = DefaultHasher::new();
                actual.hash(&mut hasher);
                hasher.finish()
            };
            assert_eq!(hash, actual_hash);
            Ok(())
        });
    }

    impl<'a> Arbitrary<'a> for FoldedValue {
        fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
            let seed: u64 = u.arbitrary()?;
            let mut rng = Mt64::new(seed);
            let num_lines = rng.gen_range(1..10);
            let mut lines = Vec::with_capacity(num_lines);
            let chars = valid_chars();
            // first line
            {
                let num_chars = rng.gen_range(1..128);
                let mut line = String::with_capacity(num_chars);
                for _ in 0..num_chars {
                    let ch = loop {
                        let ch = chars[rng.gen_range(0..chars.len())] as char;
                        if !dpkg_is_whitespace(ch) {
                            break ch;
                        }
                    };
                    line.push(ch);
                }
                lines.push(line);
            }
            for _ in 1..num_lines {
                let num_chars = rng.gen_range(1..128);
                let mut line = String::with_capacity(num_chars);
                for _ in 0..num_chars {
                    line.push(chars[rng.gen_range(0..chars.len())] as char);
                }
                while line.is_empty() || line.chars().all(dpkg_is_whitespace) || line == "." {
                    line.push(chars[rng.gen_range(0..chars.len())] as char);
                }
                lines.push(line);
            }
            Ok(Self::try_from(lines.join("\n")).unwrap())
        }
    }

    fn valid_chars() -> Vec<u8> {
        disjoint_intervals([b' ', b'/', u8::MAX])
    }

    fn dpkg_is_whitespace(ch: char) -> bool {
        ch.is_whitespace() || ch.is_control()
    }
}
