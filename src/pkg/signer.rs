use rand::rngs::OsRng;
use secp256k1::ecdsa::Signature;
use secp256k1::generate_keypair;
use secp256k1::hashes::sha256;
use secp256k1::hashes::Hash;
use secp256k1::Message;

use crate::sign::Error;
use crate::sign::Signer;
use crate::sign::Verifier;

pub type SigningKey = secp256k1::SecretKey;
pub type VerifyingKey = secp256k1::PublicKey;
pub type PackageVerifier = VerifyingKey;

pub struct PackageSigner(SigningKey);

impl PackageSigner {
    pub fn generate() -> (Self, PackageVerifier) {
        let (signing_key, verifying_key) = generate_keypair(&mut OsRng);
        (Self(signing_key), verifying_key)
    }
}

impl Signer for PackageSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let digest = sha256::Hash::hash(message);
        let message = Message::from_digest(digest.to_byte_array());
        Ok(self.0.sign_ecdsa(message).serialize_der().to_vec())
    }
}

/*
impl Signer for &PackageSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(self.sign_ecdsa(message).serialize_der().to_vec())
    }
}
*/

impl Verifier for PackageVerifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Error> {
        let signature = Signature::from_der(signature).map_err(|_| Error)?;
        let digest = sha256::Hash::hash(message);
        let message = Message::from_digest(digest.to_byte_array());
        signature.verify(&message, self).map_err(|_| Error)?;
        Ok(())
    }
}

/*
impl Verifier for &PackageVerifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Error> {
        let signature = Signature::from_bytes(signature, None).map_err(|_| Error)?;
        ksign::VerifyingKey::verify(self, message, &signature).map_err(|_| Error)?;
        Ok(())
    }
}
*/

#[cfg(test)]
mod tests {
    use std::process::Command;

    use pkcs8::AlgorithmIdentifierRef;
    use pkcs8::ObjectIdentifier;
    use pkcs8::PrivateKeyInfo;
    use tempfile::TempDir;

    use super::*;

    #[ignore]
    #[test]
    fn freebsd_pkg_pubout() {
        let (signer, verifier) = PackageSigner::generate();
        let workdir = TempDir::new().unwrap();
        let private_key_file = workdir.path().join("private-key");
        PrivateKeyInfo {
            algorithm: AlgorithmIdentifierRef {
                oid: ObjectIdentifier::from_arcs([1, 3, 101, 112]).unwrap(),
                parameters: None,
            },
            private_key: &signer.0[..],
            public_key: None, //verifier.serialize_uncompressed(),
        }
        .encrypt(&mut OsRng, &[])
        .unwrap()
        .write_der_file(private_key_file.as_path())
        .unwrap();
        assert!(Command::new("ls")
            .arg("-l")
            .arg(private_key_file.as_path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("openssl")
            .arg("asn1parse")
            .arg("-inform")
            .arg("der")
            .arg("-i")
            .arg("-in")
            .arg(private_key_file.as_path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("pkg")
            .arg("key")
            .arg("--public")
            .arg("-t")
            .arg("ecdsa")
            .arg(private_key_file.as_path())
            .status()
            .unwrap()
            .success());
    }
}
