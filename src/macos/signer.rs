use std::ops::Deref;

use pgp::composed::KeyType;
use pgp::crypto::hash::HashAlgorithm;
use pgp::packet::SignatureType;
use pgp::types::SecretKeyTrait;
use pgp::types::SignatureBytes;
use pgp::SecretKeyParamsBuilder;
use pgp::SignedPublicKey;
use pgp::SignedSecretKey;
use rand::rngs::OsRng;

use crate::sign::Error;
use crate::sign::PgpSignature;
use crate::sign::PgpSigner;
use crate::sign::PgpVerifier;
use crate::sign::Verifier;
use crate::xar::XarSigner;

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

    pub fn sign(&self, message: &[u8]) -> Result<PgpSignature, Error> {
        self.inner.sign_v2(message)
    }
}

impl XarSigner for PackageSigner {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        let s =
            PackageSigner::sign(self, data).map_err(|_| std::io::Error::other("signing failed"))?;
        let bytes = match s.into_inner().signature {
            SignatureBytes::Mpis(x) => x.into_iter().flat_map(|x| x.to_vec()).collect::<Vec<u8>>(),
            SignatureBytes::Native(x) => x,
        };
        debug_assert!(self.signature_len() == bytes.len());
        Ok(bytes)
    }

    fn signature_style(&self) -> &str {
        "RSA"
    }

    fn signature_len(&self) -> usize {
        256
    }
}

pub struct PackageVerifier {
    inner: PgpVerifier,
}

impl PackageVerifier {
    pub fn new(verifying_key: VerifyingKey) -> Self {
        Self {
            inner: PgpVerifier::new(verifying_key.into()),
        }
    }

    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Error> {
        self.inner.verify(message, signature)
    }
}

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
        let key_type = KeyType::Rsa(2048);
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
        Ok((
            SigningKey(signed_secret_key),
            VerifyingKey(signed_public_key),
        ))
    }
}

pub struct VerifyingKey(SignedPublicKey);

impl From<VerifyingKey> for SignedPublicKey {
    fn from(other: VerifyingKey) -> Self {
        other.0
    }
}

impl Deref for VerifyingKey {
    type Target = SignedPublicKey;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
