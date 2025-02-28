use std::ops::Deref;
use std::str::FromStr;

use base58::FromBase58;
use base58::ToBase58;
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use sha2::Sha512;
use wolfpack::deb;
use wolfpack::ipk;
use wolfpack::macos;
use wolfpack::pkg;
use wolfpack::rpm;
use zeroize::ZeroizeOnDrop;

use crate::Error;

/// Key material from which package and repository signing keys are derived.
#[derive(ZeroizeOnDrop)]
pub struct MasterSecretKey([u8; MASTER_SECRET_KEY_LEN]);

impl MasterSecretKey {
    pub fn generate() -> Self {
        let mut master_secret_key = [0_u8; MASTER_SECRET_KEY_LEN];
        OsRng.fill_bytes(&mut master_secret_key[..]);
        Self(master_secret_key)
    }

    #[allow(unused)]
    pub fn new(bytes: [u8; MASTER_SECRET_KEY_LEN]) -> Self {
        Self(bytes)
    }
}

impl Deref for MasterSecretKey {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0[..]
    }
}

impl AsRef<[u8]> for MasterSecretKey {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl AsRef<[u8; MASTER_SECRET_KEY_LEN]> for MasterSecretKey {
    fn as_ref(&self) -> &[u8; MASTER_SECRET_KEY_LEN] {
        &self.0
    }
}

impl FromStr for MasterSecretKey {
    type Err = MasterSecretKeyParseError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes = value
            .from_base58()
            .map_err(|_| MasterSecretKeyParseError)?
            .try_into()
            .map_err(|_| MasterSecretKeyParseError)?;
        Ok(Self(bytes))
    }
}

impl std::fmt::Display for MasterSecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_base58())
    }
}

#[derive(Debug)]
pub struct MasterSecretKeyParseError;

pub struct SigningKeyGenerator {
    hkdf: HkdfSha512,
}

impl SigningKeyGenerator {
    pub fn new(master_secret_key: &MasterSecretKey) -> Self {
        let hkdf = HkdfSha512::new(Some(&SEED[..]), &master_secret_key[..]);
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
        const MAX_ITERATIONS: usize = 1000;
        for _ in 0..MAX_ITERATIONS {
            if let Ok(signing_key) = pkg::SigningKey::from_bytes(&signing_key) {
                let verifying_key = signing_key.verifying_key();
                return Ok((signing_key, verifying_key));
            }
        }
        Err(Error::Other(format!(
            "Failed to generate `pkg` secret key after {MAX_ITERATIONS} iterations"
        )))
    }

    pub fn macos(&self) -> Result<(macos::SigningKey, macos::VerifyingKey), Error> {
        // RSA key is not a bag of bytes, so we generate it from the seed
        // using a crypto PRNG.
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

const MASTER_SECRET_KEY_LEN: usize = 64;
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
        let master_secret_key = MasterSecretKey::new([123_u8; MASTER_SECRET_KEY_LEN]);
        let gen = SigningKeyGenerator::new(&master_secret_key);
        let (signing_key_1, verifying_key_1) = gen.deb().unwrap();
        let gen = SigningKeyGenerator::new(&master_secret_key);
        let (signing_key_2, verifying_key_2) = gen.deb().unwrap();
        assert_eq!(
            signing_key_1.to_armored_string().unwrap(),
            signing_key_2.to_armored_string().unwrap(),
        );
        assert_eq!(
            verifying_key_1
                .to_armored_string(Default::default())
                .unwrap(),
            verifying_key_2
                .to_armored_string(Default::default())
                .unwrap(),
        );
    }
}
