use std::fmt::Display;
use std::fmt::Formatter;

use crate::deb::Error;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SimpleValue(String);

impl SimpleValue {
    pub fn try_from(value: String) -> Result<Self, Error> {
        if !value.chars().all(is_valid_char) {
            return Err(Error::FieldValue(value));
        }
        Ok(Self(value))
    }
}

impl Display for SimpleValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FoldedValue(String);

impl FoldedValue {
    pub fn try_from(value: String) -> Result<Self, Error> {
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
            Ok(Self(folded))
        } else {
            Ok(Self(value))
        }
    }
}

impl Display for FoldedValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_valid_char(ch: char) -> bool {
    !['\r', '\n'].contains(&ch)
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MultilineValue(String);

impl MultilineValue {
    pub fn try_from(value: String) -> Result<Self, Error> {
        let num_lines = value.lines().count();
        let mut multiline = String::with_capacity(value.len() + num_lines);
        for line in value.lines() {
            if line.chars().all(|ch| [' ', '\t'].contains(&ch)) {
                multiline.push_str(" .\n");
            } else {
                multiline.push(' ');
                multiline.push_str(line);
                multiline.push('\n');
            }
        }
        Ok(Self(multiline))
    }
}

impl Display for MultilineValue {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
