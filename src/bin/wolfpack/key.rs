use crate::Error;

use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha512;
use wolfpack::deb;

pub type Entropy = [u8; ENTROPY_LEN];

pub fn generate_entropy() -> Result<[u8; ENTROPY_LEN], Error> {
    let mut entropy = [0_u8; ENTROPY_LEN];
    OsRng.fill_bytes(&mut entropy[..]);
    Ok(entropy)
}

pub struct SigningKeyGenerator {
    hkdf: HkdfSha512,
}

impl SigningKeyGenerator {
    pub fn new(entropy: &Entropy) -> Self {
        let hkdf = HkdfSha512::new(Some(&SEED[..]), &entropy[..]);
        Self { hkdf }
    }

    pub fn deb(&self) -> Result<(deb::SigningKey, deb::VerifyingKey), Error> {
        let mut signing_key = [0_u8; 32];
        self.hkdf
            .expand(&INFO_DEB, &mut signing_key)
            .expect("The length is valid");
        let (signing_key, verifying_key) = deb::SigningKey::from_bytes(&signing_key)?;
        Ok((signing_key, verifying_key))
    }
}

type HkdfSha512 = Hkdf<Sha512>;

const ENTROPY_LEN: usize = 64;
const SEED: [u8; 13] = *b"Wolfpack seed";
const INFO_DEB: [u8; 3] = *b"deb";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deb() {
        let entropy = [123_u8; ENTROPY_LEN];
        let gen = SigningKeyGenerator::new(&entropy);
        let (signing_key_1, _verifying_key) = gen.deb().unwrap();
        let key1 = signing_key_1.to_armored_string().unwrap();
        let (signing_key_2, _verifying_key) = gen.deb().unwrap();
        let key2 = signing_key_2.to_armored_string().unwrap();
        assert_eq!(key1, key2);
    }
}
