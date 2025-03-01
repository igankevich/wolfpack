use crate::macros::define_arch_enum;

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
