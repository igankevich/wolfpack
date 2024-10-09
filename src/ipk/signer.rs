use ksign::Signature;
use ksign::IO;

use crate::sign::Error;
use crate::sign::Signer;
use crate::sign::Verifier;

pub type SigningKey = ksign::SigningKey;
pub type VerifyingKey = ksign::VerifyingKey;
pub type PackageSigner = SigningKey;
pub type PackageVerifier = VerifyingKey;

impl Signer for PackageSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(SigningKey::sign(self, message).to_bytes())
    }
}

impl Signer for &PackageSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(SigningKey::sign(self, message).to_bytes())
    }
}

impl Verifier for PackageVerifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Error> {
        let signature = Signature::from_bytes(signature, None).map_err(|_| Error)?;
        self.verify(message, &signature).map_err(|_| Error)?;
        Ok(())
    }
}
