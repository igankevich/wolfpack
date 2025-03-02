use crate::elf;
use crate::macros::define_str_enum;

// This directory https://git.openwrt.org/openwrt/openwrt/target/linux/
// contains `target.mk` files and `Makefile` files that in turn contain `ARCH`
// and `CPU_TYPE` variables. The resulting package architecture is the
// interspersion of `ARCH` and `CPU_TYPE` (if any) with an underscore (every `-`
// in `ARCH` and `CPU_TYPE` also becomes an underscore). For example, for `ARCH
// := aarch64` and `CPU_TYPE := cortex-a53` the resulting package architecture
// is `aarch64_cortex_a53`.
define_str_enum! {
    Arch,
    (Aarch64, "aarch64"),
    (Arc, "arc"),
    (Arm, "arm"),
    (Armeb, "armeb"),
    (I386, "i386"),
    (Loongarch64, "loongarch64"),
    (Mips, "mips"),
    (Mips64, "mips64"),
    (Mips64el, "mips64el"),
    (Mipsel, "mipsel"),
    (Powerpc, "powerpc"),
    (Powerpc64, "powerpc64"),
    (Riscv64, "riscv64"),
    (X86_64, "x86_64"),
    (All, "all"),
    (Noarch, "noarch"),
}

impl From<Option<elf::Target>> for Arch {
    fn from(target: Option<elf::Target>) -> Self {
        use elf::macros::*;
        match target {
            target!(Arc) => Self::Arc,
            target!(Loong, LittleEndian, Elf64) => Self::Loongarch64,
            target!(X86_64) => Self::X86_64,
            target!(I386) => Self::I386,
            target!(Aarch64) => Self::Aarch64,
            target!(Arm, BigEndian) => Self::Armeb,
            target!(Arm, LittleEndian) => Self::Arm,
            target!(Mips, LittleEndian, Elf32) => Self::Mipsel,
            target!(Mips, BigEndian, Elf32) => Self::Mips,
            target!(Mips, LittleEndian, Elf64) => Self::Mips64el,
            target!(Mips, BigEndian, Elf64) => Self::Mips64,
            // TODO MipsRs3Le, MipsX
            target!(Ppc, BigEndian) => Self::Powerpc,
            target!(Ppc64, BigEndian) => Self::Powerpc64,
            Some(other) => {
                log::warn!("No architecture mapping for ELF target \"{}\"", other);
                Self::Noarch
            }
            None => Self::Noarch,
        }
    }
}
