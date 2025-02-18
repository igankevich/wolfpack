use crate::Error;

use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use rand_chacha::ChaCha20Rng;
use sha2::Sha512;
use wolfpack::deb;
use wolfpack::ipk;
use wolfpack::macos;
use wolfpack::pkg;
use wolfpack::rpm;

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

    pub fn rpm(&self) -> Result<(rpm::SigningKey, rpm::VerifyingKey), Error> {
        let mut signing_key = [0_u8; 32];
        self.hkdf
            .expand(&INFO_RPM, &mut signing_key)
            .expect("The length is valid");
        let (signing_key, verifying_key) = rpm::SigningKey::from_bytes(&signing_key)?;
        Ok((signing_key, verifying_key))
    }

    pub fn ipk(&self) -> Result<(ipk::SigningKey, ipk::VerifyingKey), Error> {
        let mut signing_key = [0_u8; 32];
        let mut salt = [0_u8; ksign::Salt::LEN];
        let mut fingerprint = [0_u8; ksign::Fingerprint::LEN];
        for (info, bytes) in [
            (&INFO_IPK_KEY[..], &mut signing_key[..]),
            (&INFO_IPK_SALT[..], &mut salt[..]),
            (&INFO_IPK_FINGERPRINT[..], &mut fingerprint[..]),
        ] {
            self.hkdf.expand(info, bytes).expect("The length is valid");
        }
        let signing_key = ksign::ed25519::SigningKey::from_bytes(&signing_key);
        let signing_key = ipk::SigningKey::new(signing_key, salt.into(), fingerprint.into(), None);
        let verifying_key = signing_key.to_verifying_key();
        Ok((signing_key, verifying_key))
    }

    #[allow(unused)]
    pub fn pkg(&self) -> Result<(pkg::SigningKey, pkg::VerifyingKey), Error> {
        let mut signing_key = [0_u8; 32];
        self.hkdf
            .expand(&INFO_PKG, &mut signing_key)
            .expect("The length is valid");
        let (signing_key, verifying_key) = loop {
            if let Ok(signing_key) = pkg::SigningKey::from_bytes(&signing_key) {
                let verifying_key = signing_key.verifying_key();
                break (signing_key, verifying_key);
            }
        };
        Ok((signing_key, verifying_key))
    }

    pub fn macos(&self) -> Result<(macos::SigningKey, macos::VerifyingKey), Error> {
        // RSA key is not a bag of bytes, so we generate it from the seed
        // using a crypto PRNG.
        use rand_chacha::rand_core::SeedableRng;
        let mut seed = [0_u8; 32];
        self.hkdf
            .expand(&INFO_MACOS_SEED, &mut seed)
            .expect("The length is valid");
        let mut rng = ChaCha20Rng::from_seed(seed);
        let signing_key = macos::SigningKey::new(&mut rng, 2048).map_err(|_| Error::Sign)?;
        let verifying_key = signing_key.to_public_key();
        Ok((signing_key, verifying_key))
    }
}

type HkdfSha512 = Hkdf<Sha512>;

const ENTROPY_LEN: usize = 64;
const SEED: [u8; 13] = *b"Wolfpack seed";

const INFO_DEB: [u8; 3] = *b"deb";
const INFO_RPM: [u8; 3] = *b"rpm";
const INFO_IPK_KEY: [u8; 7] = *b"ipk/key";
const INFO_IPK_SALT: [u8; 8] = *b"ipk/salt";
const INFO_IPK_FINGERPRINT: [u8; 15] = *b"ipk/fingerprint";
#[allow(unused)]
const INFO_PKG: [u8; 3] = *b"pkg";
const INFO_MACOS_SEED: [u8; 10] = *b"macos/seed";

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
