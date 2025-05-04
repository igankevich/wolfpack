use chrono::DateTime;
use std::collections::hash_map::Entry::*;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

use serde::Deserialize;
use serde::Serialize;

use crate::deb::Error;
use crate::deb::FieldName;
use crate::deb::Value;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(test, derive(arbitrary::Arbitrary))]
pub struct Fields {
    fields: HashMap<FieldName, Value>,
}

impl Fields {
    pub fn new() -> Self {
        Self {
            fields: Default::default(),
        }
    }

    pub fn remove_any(&mut self, name: &'static str) -> Result<Value, Error> {
        self.fields
            .remove(&FieldName::new_unchecked(name))
            .ok_or(Error::MissingField(name))
    }

    pub fn remove<T: FromStr>(&mut self, name: &'static str) -> Result<T, Error>
    where
        <T as FromStr>::Err: Display,
    {
        let value = self
            .fields
            .remove(&FieldName::new_unchecked(name))
            .ok_or(Error::MissingField(name))?;
        value
            .as_str()
            .parse::<T>()
            .map_err(|e| Error::FieldValue(name, value.to_string(), e.to_string()))
    }

    pub fn remove_some<T: FromStr>(&mut self, name: &'static str) -> Result<Option<T>, Error>
    where
        <T as FromStr>::Err: Display,
    {
        self.fields
            .remove(&FieldName::new_unchecked(name))
            .map(|value| {
                value
                    .as_str()
                    .parse::<T>()
                    .map_err(|e| Error::FieldValue(name, value.to_string(), e.to_string()))
            })
            .transpose()
    }

    pub fn remove_system_time(&mut self, name: &'static str) -> Result<Option<SystemTime>, Error> {
        let Some(value) = self.fields.remove(&FieldName::new_unchecked(name)) else {
            return Ok(None);
        };
        match parse_date(value.as_str()) {
            Ok(date) => Ok(Some(date)),
            Err(e) => {
                log::error!("Failed to parse date {:?}: {}", value, e);
                Ok(None)
            }
        }
    }

    pub fn remove_hashes<H: FromStr>(
        &mut self,
        name: &'static str,
    ) -> Result<HashMap<PathBuf, (H, u64)>, Error> {
        let mut hashes = HashMap::new();
        let Some(value) = self.fields.remove(&FieldName::new_unchecked(name)) else {
            return Ok(hashes);
        };
        for line in value.as_str().lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut values = line.split_whitespace();
            let hash: H = values
                .next()
                .ok_or_else(|| Error::other("file hash is missing"))?
                .parse()
                .map_err(|_| Error::other("failed to parse file hash"))?;
            let size: u64 = values
                .next()
                .ok_or_else(|| Error::other("file size is missing"))?
                .parse::<u64>()
                .map_err(|_| Error::other("failed to parse file size"))?;
            let path: PathBuf = values
                .next()
                .ok_or_else(|| Error::other("file path is missing"))?
                .into();
            hashes.insert(path, (hash, size));
        }
        Ok(hashes)
    }

    pub fn clear(&mut self) {
        self.fields.clear();
    }
}

impl Default for Fields {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Fields {
    type Target = HashMap<FieldName, Value>;

    fn deref(&self) -> &Self::Target {
        &self.fields
    }
}

impl FromStr for Fields {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut state = ParserStatus::Initial;
        let mut fields = Fields::new();
        let mut lines = value.lines();
        let mut prev_line = lines.next();
        for line in lines {
            if line.starts_with('#') {
                continue;
            }
            if line.chars().all(char::is_whitespace) {
                return Err(Error::Package("empty line".into()));
            }
            state = state.advance(prev_line, Some(line), &mut fields)?;
            prev_line = Some(line);
        }
        if prev_line.is_some() {
            state = state.advance(prev_line, None, &mut fields)?;
        }
        state.advance(None, None, &mut fields)?;
        Ok(fields)
    }
}

#[derive(Debug)]
enum ParserStatus {
    Initial,
    Reading(FieldName, String, usize, bool),
}

impl ParserStatus {
    fn advance(
        self,
        line: Option<&str>,
        next_line: Option<&str>,
        fields: &mut Fields,
    ) -> Result<Self, Error> {
        //eprintln!("{self:?} {line:?}");
        let state = match (self, line) {
            (ParserStatus::Initial, Some(line)) => {
                let mut iter = line.splitn(2, ':');
                let name = iter.next().ok_or_else(|| Error::Package(line.into()))?;
                let value = iter.next().ok_or_else(|| Error::Package(line.into()))?;
                let value = value.trim_start();
                let name: FieldName = name.parse()?;
                if next_line
                    .map(|l| l.starts_with([' ', '\t']))
                    .unwrap_or(false)
                {
                    // Multiline/folded value.
                    ParserStatus::Reading(name, value.into(), 1, false)
                } else {
                    // Simple value.
                    let value = Value::Simple(value.parse()?);
                    match fields.fields.entry(name) {
                        Occupied(o) => return Err(Error::DuplicateField(o.key().to_string())),
                        Vacant(v) => {
                            v.insert(value);
                        }
                    }
                    ParserStatus::Initial
                }
            }
            (ParserStatus::Reading(name, mut value, num_lines, has_empty_lines), Some(line)) => {
                let has_empty_lines = has_empty_lines || line == " ." || line == "\t.";
                value.push('\n');
                value.push_str(line);
                if next_line
                    .map(|l| l.starts_with([' ', '\t']))
                    .unwrap_or(false)
                {
                    // Continue reading multiline/foded value.
                    ParserStatus::Reading(name, value, num_lines + 1, has_empty_lines)
                } else {
                    // This is the last line of multiline/folded value.
                    let value = if has_empty_lines || is_multiline(&name) {
                        Value::Multiline(value.into())
                    } else {
                        Value::Folded(value.try_into()?)
                    };
                    match fields.fields.entry(name) {
                        Occupied(o) => return Err(Error::DuplicateField(o.key().to_string())),
                        Vacant(v) => {
                            v.insert(value);
                        }
                    }
                    ParserStatus::Initial
                }
            }
            (ParserStatus::Reading(..), None) => unreachable!(),
            (state @ ParserStatus::Initial, None) => state,
        };
        Ok(state)
    }
}

fn is_multiline(name: &FieldName) -> bool {
    name == "description" || name == "md5sum" || name == "sha256" || name == "sha1"
}

fn parse_date(value: &str) -> Result<SystemTime, chrono::format::ParseError> {
    // TODO parse time zones
    let value = value.replace("UTC", "+0000");
    let mut first_error = None;
    for format in RFC_2822_FORMATS {
        match DateTime::parse_from_str(&value, format) {
            Ok(t) => return Ok(t.into()),
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    Err(first_error.expect("At least one iteration of the loop was executed"))
}

const RFC_2822_FORMATS: [&str; 4] = [
    "%a, %d %b %Y %H:%M:%S%z",
    "%a, %_d %b %Y %H:%M:%S%z",
    "%a, %d %b %Y %_H:%M:%S%z",
    "%a, %_d %b %Y %_H:%M:%S%z",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date() {
        match parse_date("Thu, 25 Apr 2024 15:10:33 +0000") {
            Ok(..) => {}
            Err(e) => panic!("{e}"),
        }
        match parse_date("Sun, 04 May 2025  2:20:26 UTC") {
            Ok(..) => {}
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn test_parse_fields() {
        let input = "\
Architecture: amd64
Version: 0.9.6-4
Built-Using: rust-ahash-0.7 (= 0.7.7-2), rust-nix (= 0.26.2-1), rust-option-ext (= 0.2.0-1), rustc (= 1.75.0+dfsg0ubuntu1-0ubuntu1)
Multi-Arch: allowed
Priority: optional
Section: universe/utils
Origin: Ubuntu
Maintainer: Ubuntu Developers <ubuntu-devel-discuss@lists.ubuntu.com>
Original-Maintainer: Jonas Smedegaard <dr@jones.dk>
Bugs: https://bugs.launchpad.net/ubuntu/+filebug
Installed-Size: 4799
Depends: libc6 (>= 2.38), libgcc-s1 (>= 4.2)
Suggests: bash-completion
Filename: pool/universe/b/btm/btm_0.9.6-4_amd64.deb
Size: 1607224
MD5sum: 5af6a25fa3b1bb766aecbc7a290670e7
SHA1: 5a8e3563ffc958d5a7b1dff8b97300eb03b1cd02
SHA256: 62b3c95436097e45edeebd72396831938df40de055d2e0dd9fcf276639314799
SHA512: 29806c67d9f74461eedadb488a94bf66d478cda15f33c44fbbc2c710902455fff875d38cada331c1aea84fc2fc6d7bf80aecb0c415273a70751a7ce543ff5518
Homepage: https://clementtsang.github.io/bottom
Description: customizable graphical process/system monitor for the terminal
X-Cargo-Built-Using:
 rust-addr2line (= 0.21.0-2), rust-adler (= 1.0.2-2), rust-ahash-0.7 (= 0.7.7-2), rust-aho-corasick (= 1.1.2-1), rust-anstream (= 0.6.7-1), rust-anstyle (= 1.0.4-1), rust-anstyle-parse (= 0.2.1-1), rust-anstyle-query (= 1.0.0-1), rust-anyhow (= 1.0.75-1), rust-assert-cmd (= 2.0.12-1), rust-backtrace (= 0.3.69-2), rust-bitflags-1 (= 1.3.2-5), rust-bitflags (= 2.4.2-1), rust-bstr (= 1.7.0-2build1), rust-cassowary (= 0.3.0-2), rust-cfg-if (= 1.0.0-1), rust-clap-builder (= 4.4.18-1), rust-clap (= 4.4.18-1), rust-clap-lex (= 0.6.0-2), rust-colorchoice (= 1.0.0-1), rust-concat-string (= 1.0.1-1), rust-crossbeam-deque (= 0.8.5-1), rust-crossbeam-epoch (= 0.9.18-1), rust-crossbeam-utils (= 0.8.19-1), rust-crossterm (= 0.27.0-3), rust-ctrlc (= 3.4.2-1), rust-difflib (= 0.4.0-1), rust-dirs (= 5.0.1-1), rust-dirs-sys (= 0.4.1-1), rust-doc-comment (= 0.3.3-1), rust-either (= 1.9.0-1), rust-float-cmp (= 0.9.0-1), rust-getrandom (= 0.2.10-1), rust-gimli (= 0.28.1-2), rust-hashbrown (= 0.12.3-1), rust-humantime (= 2.1.0-1), rust-indexmap (= 1.9.3-2), rust-itertools (= 0.10.5-1), rust-itoa (= 1.0.9-1), rust-kstring (= 2.0.0-1), rust-lazycell (= 1.3.0-3), rust-libc (= 0.2.152-1), rust-libloading (= 0.7.4-1), rust-linux-raw-sys (= 0.4.12-1), rust-lock-api (= 0.4.11-1), rust-log (= 0.4.20-2), rust-memchr (= 2.7.1-1), rust-miniz-oxide (= 0.7.1-1), rust-mio (= 0.8.10-1), rust-nix (= 0.26.2-1), rust-normalize-line-endings (= 0.3.0-1), rust-num-traits (= 0.2.15-1), rust-nvml-wrapper (= 0.9.0-1), rust-nvml-wrapper-sys (= 0.7.0-1), rust-object (= 0.32.2-1), rust-once-cell (= 1.19.0-1), rust-option-ext (= 0.2.0-1), rust-parking-lot-core (= 0.9.9-1), rust-parking-lot (= 0.12.1-2build1), rust-predicates-core (= 1.0.6-1), rust-predicates (= 3.0.3-1), rust-predicates-tree (= 1.0.7-1), rust-ratatui (= 0.23.0-4), rust-rayon-core (= 1.12.1-1), rust-rayon (= 1.8.1-1), rust-regex-automata (= 0.4.3-1build2), rust-regex (= 1.10.2-2build2), rust-regex-syntax (= 0.8.2-1), rust-rustc-demangle (= 0.1.21-1), rust-rustix (= 0.38.30-1), rust-scopeguard (= 1.1.0-1), rust-serde (= 1.0.195-1), rust-serde-spanned (= 0.6.4-1), rust-signal-hook (= 0.3.17-1), rust-signal-hook-mio (= 0.2.3-2), rust-signal-hook-registry (= 1.4.0-1), rust-smallvec (= 1.11.2-1), rust-starship-battery (= 0.8.2-1), rust-static-assertions (= 1.1.0-1), rust-strsim (= 0.10.0-1), rust-strum (= 0.25.0-1), rust-sysinfo (= 0.28.4-4), rust-terminal-size (= 0.3.0-2), rust-termtree (= 0.4.1-1), rust-thiserror (= 1.0.50-1), rust-time-core (= 0.1.1-1), rust-time (= 0.3.23-2), rust-toml-datetime (= 0.6.5-1), rust-toml-edit (= 0.21.0-2), rust-typenum (= 1.16.0-2), rust-unicode-segmentation (= 1.10.1-1), rust-unicode-width (= 0.1.11-1), rust-uom (= 0.35.0-1), rust-utf8parse (= 0.2.1-1), rust-wait-timeout (= 0.2.0-1), rust-winnow (= 0.5.15-1), rustc (= 1.75.0+dfsg0ubuntu1-0ubuntu1),
Description-md5: e39e31ca350d6a0cb1ee1479936064f3
";
        Fields::from_str(input).unwrap();
    }

    #[test]
    fn test_parse_fields_2() {
        let input = "\
Origin: Ubuntu
Label: Ubuntu
Suite: noble
Version: 24.04
Codename: noble
Date: Thu, 25 Apr 2024 15:10:33 UTC
Architectures: amd64 arm64 armhf i386 ppc64el riscv64 s390x
Components: main restricted universe multiverse
Description: Ubuntu Noble 24.04
";
        eprintln!("{:#?}", Fields::from_str(input).unwrap());
    }
}
