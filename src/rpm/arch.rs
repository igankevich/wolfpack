use crate::elf;
use crate::macros::define_str_enum;

define_str_enum! {
    Arch,
    (X86_64, "x86_64"),
    (Aarch64, "aarch64"),
    (Arm, "arm"),
    (Armhfp, "armhfp"),
    (I386, "i386"),
    (Mips64el, "mips64el"),
    (Mipsel, "mipsel"),
    (Mips64, "mips64"),
    (Mips, "mips"),
    (Ppc64, "ppc64"),
    (Ppc64le, "ppc64le"),
    (S390x, "s390x"),
    (Sparc, "sparc"),
    (Sparc64, "sparc64"),
    (Noarch, "noarch"),
}

impl From<Option<elf::Target>> for Arch {
    fn from(target: Option<elf::Target>) -> Self {
        use elf::macros::*;
        use elf::Flags;
        use elf::Machine;
        use elf::Target;
        match target {
            target!(X86_64) => Self::X86_64,
            target!(I386) => Self::I386,
            target!(Aarch64) => Self::Aarch64,
            Some(Target {
                machine: Machine::Arm,
                flags,
                ..
            }) if flags.contains(Flags::ARM_SOFT_FLOAT) => Self::Arm,
            Some(Target {
                machine: Machine::Arm,
                flags,
                ..
            }) if flags.contains(Flags::ARM_HARD_FLOAT) => Self::Armhfp,
            target!(Mips, LittleEndian, Elf32) => Self::Mipsel,
            target!(Mips, BigEndian, Elf32) => Self::Mips,
            target!(Mips, LittleEndian, Elf64) => Self::Mips64el,
            target!(Mips, BigEndian, Elf64) => Self::Mips64,
            // TODO MipsRs3Le, MipsX
            target!(Ppc64, BigEndian) => Self::Ppc64,
            target!(Ppc64, LittleEndian) => Self::Ppc64le,
            target!(S390) => Self::S390x,
            target!(Sparc) => Self::Sparc,
            target!(Sparc32Plus) => Self::Sparc,
            target!(Sparcv9) => Self::Sparc64,
            Some(other) => {
                log::warn!("No architecture mapping for ELF target \"{}\"", other);
                Self::Noarch
            }
            None => Self::Noarch,
        }
    }
}
