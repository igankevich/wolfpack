use std::collections::HashSet;

use crate::deb::SimpleValue;
use crate::deb::Value;
use crate::elf;
use crate::macros::define_str_enum;

define_str_enum! {
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
    (Ppc64, "ppc64"),
    (Ppc64el, "ppc64el"),
    (S390x, "s390x"),
    (Sparc32, "sparc32"),
    (Sparc64, "sparc64"),
    (All, "all"),
}

impl From<Option<elf::Target>> for Arch {
    fn from(target: Option<elf::Target>) -> Self {
        use elf::macros::*;
        use elf::Flags;
        use elf::Machine;
        use elf::Target;
        match target {
            target!(X86_64) => Self::Amd64,
            target!(I386) => Self::I386,
            target!(Aarch64) => Self::Arm64,
            Some(Target {
                machine: Machine::Arm,
                flags,
                ..
            }) if flags.contains(Flags::ARM_SOFT_FLOAT) => Self::Armel,
            Some(Target {
                machine: Machine::Arm,
                flags,
                ..
            }) if flags.contains(Flags::ARM_HARD_FLOAT) => Self::Armhf,
            target!(Mips, LittleEndian, Elf32) => Self::Mipsel,
            target!(Mips, BigEndian, Elf32) => Self::Mips,
            target!(Mips, LittleEndian, Elf64) => Self::Mips64el,
            target!(Mips, BigEndian, Elf64) => Self::Mips64,
            // TODO MipsRs3Le, MipsX
            target!(Ppc64, BigEndian) => Self::Ppc64,
            target!(Ppc64, LittleEndian) => Self::Ppc64el,
            target!(S390) => Self::S390x,
            target!(Sparc) => Self::Sparc32,
            target!(Sparc32Plus) => Self::Sparc32,
            target!(Sparcv9) => Self::Sparc64,
            Some(other) => {
                log::warn!("No architecture mapping for ELF target \"{}\"", other);
                Self::All
            }
            None => Self::All,
        }
    }
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
