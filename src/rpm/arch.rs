use crate::macros::define_arch_enum;
use crate::macros::define_arch_try_from;

define_arch_enum! {
    Arch,
    (X86_64, "x86_64"),
    (Aarch64, "aarch64"),
    (Armhfp, "armhfp"),
    (I386, "i386"),
    (Mips64el, "mips64el"),
    (Mipsel, "mipsel"),
    (Noarch, "noarch"),
}

define_arch_try_from! {
    crate::wolf::Arch,
    Arch,
    (Amd64, X86_64),
    (Arm64, Aarch64),
    (Armhf, Armhfp),
    (I386, I386),
    (Mips64el, Mips64el),
    (Mipsel, Mipsel),
}
