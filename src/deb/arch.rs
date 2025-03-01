use std::collections::HashSet;

use crate::deb::SimpleValue;
use crate::deb::Value;
use crate::macros::define_arch_enum;
use crate::macros::define_arch_from;

define_arch_enum! {
    Arch,
    (Amd64, "amd64"),
    (Arm64, "arm64"),
    (Armel, "armel"),
    (Armhf, "armhf"),
    (I386, "i386"),
    (Mips, "mips"),
    (Mipsel, "mipsel"),
    (Mips64, "mips64"),
    (Mips64el, "mips64el"),
    (Ppc64el, "ppc64el"),
    (S390x, "s390x"),
    (All, "all"),
}

define_arch_from! {
    crate::wolf::Arch,
    Arch,
    (Amd64, Amd64),
    (Arm64, Arm64),
    (Armel, Armel),
    (Armhf, Armhf),
    (I386, I386),
    (Mips, Mips),
    (Mipsel, Mipsel),
    (Mips64, Mips64),
    (Mips64el, Mips64el),
    (Ppc64el, Ppc64el),
    (S390x, S390x),
    (All, All),
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
