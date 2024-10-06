use pgp::crypto::hash::HashAlgorithm;
use pgp::packet::SignatureType;
use pgp::SignedSecretKey;

use crate::sign::Error;
use crate::sign::PgpSigner;
use crate::sign::PgpVerifier;
use crate::sign::Signer;

pub struct PackageSigner {
    inner: PgpSigner,
}

impl PackageSigner {
    pub fn new(signing_key: SignedSecretKey) -> Self {
        Self {
            inner: PgpSigner::new(signing_key, SignatureType::Binary, HashAlgorithm::SHA2_256),
        }
    }
}

impl Signer for PackageSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        self.inner.sign(message)
    }
}

pub type PackageVerifier = PgpVerifier;
