use crate::macros::define_arch_enum;
use crate::macros::define_arch_try_from;

// This directory https://git.openwrt.org/openwrt/openwrt/target/linux/
// contains `target.mk` files and `Makefile` files that in turn contain `ARCH`
// and `CPU_TYPE` variables. The resulting package architecture is the
// interspersion of `ARCH` and `CPU_TYPE` (if any) with an underscore (every `-`
// in `ARCH` and `CPU_TYPE` also becomes an underscore). For example, for `ARCH
// := aarch64` and `CPU_TYPE := cortex-a53` the resulting package architecture
// is `aarch64_cortex_a53`.
define_arch_enum! {
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

define_arch_try_from! {
    crate::wolf::Arch,
    Arch,
    (Amd64, X86_64),
    (Arm64, Aarch64),
    (Armel, Arm),
    (Armhf, Arm),
    (I386, I386),
    (Mips, Mips),
    (Mipsel, Mipsel),
    (Mips64, Mips64),
    (Mips64el, Mips64el),
    //(Ppc64el, ),
    //(S390x, ),
    (All, All),
}
