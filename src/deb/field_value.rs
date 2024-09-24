use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

use crate::deb::Error;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SimpleValue(String);

impl SimpleValue {
    pub fn try_from(value: String) -> Result<Self, Error> {
        validate_simple_value(&value)?;
        Ok(Self(value))
    }
    
    pub fn from_folded(value: String) -> Self {
        if value.chars().any(|ch| char::is_whitespace(ch) && ch != ' ') {
            let mut folded = String::with_capacity(value.len());
            let mut words = value.split_whitespace();
            if let Some(word) = words.next() {
                folded.push_str(word);
            }
            for word in words {
                folded.push(' ');
                folded.push_str(word);
            }
            Self(folded)
        } else {
            Self(value)
        }
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for SimpleValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for SimpleValue {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_string())
    }
}

impl From<SimpleValue> for String {
    fn from(other: SimpleValue) -> String {
        other.0
    }
}

fn validate_simple_value(value: &str) -> Result<(), Error> {
    if !value.chars().all(is_valid_char) {
        return Err(Error::FieldValue(value.to_string()));
    }
    Ok(())
}

fn is_valid_char(ch: char) -> bool {
    !['\r', '\n'].contains(&ch)
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MultilineValue(String);

impl MultilineValue {
    pub fn from(value: String) -> Self {
        Self(value)
    }
}

impl Display for MultilineValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let mut lines = self.0.lines();
        if let Some(line) = lines.next() {
            writeln!(f, "{}", line)?;
        }
        for line in lines {
            if line.chars().all(|ch| [' ', '\t'].contains(&ch)) {
                writeln!(f, " .")?;
            } else {
                writeln!(f, " {}", line)?;
            }
        }
        Ok(())
    }
}

impl FromStr for MultilineValue {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut multiline = String::with_capacity(value.len());
        for line in value.lines() {
            if line.starts_with([' ', '\t']) {
                multiline.push_str(&line[1..]);
                multiline.push('\n');
            } else if line == " ." {
                multiline.push('\n');
            } else {
                multiline.push_str(line);
                multiline.push('\n');
            }
        }
        Ok(Self::from(multiline))
    }
}
