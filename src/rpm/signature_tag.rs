#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SignatureTag {
    Signatures = 62,
    Size = 1000,
    LeMd5_1 = 1001,
    Pgp = 1002,
    LeMd5_2 = 1003,
    Md5 = 1004,
    Gpg = 1005,
    Pgp5 = 1006,
    PayloadSize = 1007,
    ReservedSpace = 1008,
    BadSha1_1 = BASE + 8,
    BadSha1_2 = BASE + 9,
    Dsa = BASE + 11,
    Rsa = BASE + 12,
    Sha1 = BASE + 13,
    LongSize = BASE + 14,
    LongArchiveSize = BASE + 15,
    Sha256 = BASE + 17,
    FileSignatures = BASE + 18,
    FileSignatureLength = BASE + 19,
    VeritySignatures = BASE + 20,
    VeritySignatureAlgo = BASE + 21,
    Other(u32),
}

impl From<u32> for SignatureTag {
    fn from(other: u32) -> Self {
        use SignatureTag::*;
        match other {
            62 => Signatures,
            1000 => Size,
            1001 => LeMd5_1,
            1002 => Pgp,
            1003 => LeMd5_2,
            1004 => Md5,
            1005 => Gpg,
            1006 => Pgp5,
            1007 => PayloadSize,
            1008 => ReservedSpace,
            other if other == BASE + 8 => BadSha1_1,
            other if other == BASE + 9 => BadSha1_2,
            other if other == BASE + 11 => Dsa,
            other if other == BASE + 12 => Rsa,
            other if other == BASE + 13 => Sha1,
            other if other == BASE + 14 => LongSize,
            other if other == BASE + 15 => LongArchiveSize,
            other if other == BASE + 17 => Sha256,
            other if other == BASE + 18 => FileSignatures,
            other if other == BASE + 19 => FileSignatureLength,
            other if other == BASE + 20 => VeritySignatures,
            other if other == BASE + 21 => VeritySignatureAlgo,
            other => Other(other),
        }
    }
}

const BASE: u32 = 256;
