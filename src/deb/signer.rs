use pgp::composed::KeyType;
use pgp::crypto::hash::HashAlgorithm;
use pgp::packet::SignatureType;
use pgp::types::SecretKeyTrait;
use pgp::SecretKeyParamsBuilder;
use pgp::SignedSecretKey;
use rand::rngs::OsRng;

use crate::sign::Error;
use crate::sign::PgpSignature;
use crate::sign::PgpSigner;
use crate::sign::Signer;
use crate::sign::Verifier;
use crate::sign::VerifierV2;

pub use crate::sign::PgpSignature as Signature;
pub use crate::sign::PgpVerifyingKey as VerifyingKey;

pub struct PackageSigner {
    inner: PgpSigner,
}

impl PackageSigner {
    pub fn new(signing_key: SigningKey) -> Self {
        Self {
            inner: PgpSigner::new(
                signing_key.into(),
                SignatureType::Binary,
                HashAlgorithm::SHA2_256,
            ),
        }
    }
}

impl Signer for PackageSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        self.inner.sign(message)
    }
}

pub struct PackageVerifier {
    verifying_keys: Vec<VerifyingKey>,
    no_verify: bool,
}

impl PackageVerifier {
    pub fn new(verifying_key: VerifyingKey) -> Self {
        Self::new_v2(vec![verifying_key])
    }

    pub fn new_v2(verifying_keys: Vec<VerifyingKey>) -> Self {
        Self {
            verifying_keys,
            no_verify: false,
        }
    }

    pub fn none() -> Self {
        Self {
            verifying_keys: Default::default(),
            no_verify: true,
        }
    }
}

impl Verifier for PackageVerifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Error> {
        if self.no_verify {
            return Ok(());
        }
        let signature = PgpSignature::read_armored_one(signature).map_err(|_| Error)?;
        VerifyingKey::verify_against_any(self.verifying_keys.iter(), message, &signature)
    }

    fn verify_any<I, S>(&self, message: &[u8], signatures: I) -> Result<(), Error>
    where
        I: Iterator<Item = S>,
        S: AsRef<[u8]>,
    {
        if self.no_verify {
            return Ok(());
        }
        for sig in signatures {
            if self.verify(message, sig.as_ref()).is_ok() {
                return Ok(());
            }
        }
        Err(Error)
    }
}

#[derive(Clone)]
pub struct SigningKey(SignedSecretKey);

impl From<SigningKey> for SignedSecretKey {
    fn from(other: SigningKey) -> Self {
        other.0
    }
}

impl SigningKey {
    pub fn generate(user_id: String) -> Result<(SigningKey, VerifyingKey), Error> {
        use pgp::crypto::aead::AeadAlgorithm::*;
        use pgp::crypto::hash::HashAlgorithm::*;
        use pgp::crypto::sym::SymmetricKeyAlgorithm::*;
        use pgp::types::CompressionAlgorithm::*;
        let key_type = KeyType::EdDSALegacy;
        let mut key_params = SecretKeyParamsBuilder::default();
        key_params
            .key_type(key_type)
            .can_encrypt(false)
            .can_certify(false)
            .can_sign(true)
            .primary_user_id(user_id)
            .preferred_symmetric_algorithms([AES256].as_slice().into())
            .preferred_hash_algorithms([SHA2_512].as_slice().into())
            .preferred_compression_algorithms([ZLIB, BZip2, ZIP].as_slice().into())
            .preferred_aead_algorithms([(AES256, Gcm)].as_slice().into());
        let secret_key_params = key_params.build().map_err(|_| Error)?;
        let secret_key = secret_key_params.generate(OsRng).map_err(|_| Error)?;
        let signed_secret_key = secret_key.sign(OsRng, String::new).map_err(|_| Error)?;
        let signed_public_key = signed_secret_key
            .public_key()
            .sign(OsRng, &signed_secret_key, String::new)
            .map_err(|_| Error)?;
        Ok((SigningKey(signed_secret_key), signed_public_key.into()))
    }
}
