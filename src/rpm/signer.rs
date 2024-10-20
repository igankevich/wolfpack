use pgp::crypto::hash::HashAlgorithm;
use pgp::packet::SignatureType;

use crate::sign::Error;
use crate::sign::PgpSigner;
use crate::sign::PgpVerifier;
use crate::sign::Signer;
use crate::sign::Verifier;

pub struct PackageSigner {
    inner: PgpSigner,
}

impl PackageSigner {
    pub fn new(signing_key: SigningKey) -> Self {
        Self {
            inner: PgpSigner::new(
                signing_key.into(),
                SignatureType::Binary,
                HashAlgorithm::SHA2_512,
            ),
        }
    }

    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        self.inner.sign(message)
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

pub type SigningKey = crate::deb::SigningKey;
pub type VerifyingKey = crate::deb::VerifyingKey;
