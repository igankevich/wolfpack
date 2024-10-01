use std::cmp::Ordering;
use std::fmt::Display;
use std::fmt::Formatter;
use std::hash::Hash;
use std::hash::Hasher;

#[derive(Clone, Debug)]
pub struct FoldedValue(pub String);

impl FoldedValue {
    pub(crate) fn words(&self) -> impl Iterator<Item = &str> {
        self.0.split_whitespace()
    }
}

impl PartialEq for FoldedValue {
    fn eq(&self, other: &Self) -> bool {
        self.0.split_whitespace().eq(other.0.split_whitespace())
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

impl From<String> for FoldedValue {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<FoldedValue> for String {
    fn from(v: FoldedValue) -> Self {
        v.0
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;

    use arbitrary::Arbitrary;
    use arbitrary::Unstructured;
    use arbtest::arbtest;

    use super::*;
    use crate::deb::SimpleValue;

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
            let actual: FoldedValue = string.clone().into();
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
            Ok(Self(u.arbitrary()?))
        }
    }
}
