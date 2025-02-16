use crate::macros::define_arch_enum;

define_arch_enum! {
    Arch,
    (X86_64, "x86_64"),
    (Aarch64, "aarch64"),
    (Armhfp, "armhfp"),
}
