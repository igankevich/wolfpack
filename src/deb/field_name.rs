use std::cmp::Ordering;
use std::fmt::Display;
use std::fmt::Formatter;
use std::hash::Hash;
use std::hash::Hasher;
use std::str::FromStr;

use crate::deb::Error;

#[derive(Clone, Debug)]
pub struct FieldName(String);

impl FieldName {
    pub fn try_from(name: String) -> Result<Self, Error> {
        if !(name.len() >= 2
            && is_valid_first_char(name.as_bytes()[0])
            && name.as_bytes().iter().all(is_valid_char))
        {
            return Err(Error::FieldName(name));
        }
        Ok(Self(name))
    }

    pub(crate) fn new_unchecked(name: &'static str) -> Self {
        Self(name.to_string())
    }
}

impl PartialEq for FieldName {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl PartialEq<str> for FieldName {
    fn eq(&self, other: &str) -> bool {
        self.0.eq_ignore_ascii_case(other)
    }
}

impl Eq for FieldName {}

impl PartialOrd for FieldName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FieldName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.to_lowercase().cmp(&other.0.to_lowercase())
    }
}

impl Hash for FieldName {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.0.to_ascii_lowercase().hash(state);
    }
}

impl Display for FieldName {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for FieldName {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_string())
    }
}

fn is_valid_char(ch: &u8) -> bool {
    (b'!'..=b'9').contains(ch) || (b';'..=b'~').contains(ch)
}

fn is_valid_first_char(ch: u8) -> bool {
    ![b'#', b'-'].contains(&ch)
}

#[cfg(test)]
impl<'a> arbitrary::Arbitrary<'a> for FieldName {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let valid_chars: Vec<_> = (b'!'..=(b'#' - 1))
            .chain((b'#' + 1)..=(b'-' - 1))
            .chain((b'-' + 1)..=b'9')
            .chain(b';'..=b'~')
            .collect();
        let len = u.arbitrary_len::<u8>()?.max(2);
        let mut string = Vec::with_capacity(len);
        for _ in 0..len {
            string.push(*u.choose(&valid_chars)?);
        }
        let string = String::from_utf8(string).unwrap();
        Ok(Self::try_from(string).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;

    use arbtest::arbtest;

    use super::*;

    #[test]
    fn invalid_names() {
        assert!("#hello".parse::<FieldName>().is_err());
        assert!("-hello".parse::<FieldName>().is_err());
        assert!("".parse::<FieldName>().is_err());
    }

    #[test]
    fn lower_case() {
        arbtest(|u| {
            let name: FieldName = u.arbitrary()?;
            let lowercase_name = FieldName::try_from(name.0.to_lowercase()).unwrap();
            assert_eq!(name, lowercase_name);
            assert_eq!(name.cmp(&lowercase_name), Ordering::Equal);
            assert_eq!(lowercase_name.cmp(&name), Ordering::Equal);
            let hash = {
                let mut hasher = DefaultHasher::new();
                name.hash(&mut hasher);
                hasher.finish()
            };
            let lowercase_hash = {
                let mut hasher = DefaultHasher::new();
                lowercase_name.hash(&mut hasher);
                hasher.finish()
            };
            assert_eq!(hash, lowercase_hash);
            Ok(())
        });
    }
}
