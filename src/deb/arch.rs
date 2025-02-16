use std::collections::HashSet;

use crate::deb::SimpleValue;
use crate::deb::Value;
use crate::macros::define_arch_enum;

define_arch_enum! {
    Arch,
    (Amd64, "amd64"),
    (Arm64, "arm64"),
    (Armel, "armel"),
    (Armhf, "armhf"),
    (I386, "i386"),
    (Mips64el, "mips64el"),
    (Mipsel, "mipsel"),
    (Ppc64el, "ppc64el"),
    (S390x, "s390x"),
}

impl TryFrom<Value> for HashSet<Arch> {
    type Error = std::io::Error;
    fn try_from(other: Value) -> Result<Self, Self::Error> {
        let mut arches = HashSet::new();
        for word in other.as_str().split_whitespace() {
            arches.insert(word.parse()?);
        }
        Ok(arches)
    }
}

impl From<Arch> for SimpleValue {
    fn from(other: Arch) -> Self {
        unsafe { Self::new_unchecked(other.to_string()) }
    }
}
