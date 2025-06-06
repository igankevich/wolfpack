use std::ops::Deref;

use blake2b_simd::blake2b;
use der::asn1::Any;
use der::asn1::BitString;
use der::Decode;
use der::Encode;
use pkcs8::ObjectIdentifier;
use pkcs8::SubjectPublicKeyInfo;
use rand::rngs::OsRng;
use secp256k1::generate_keypair;
use secp256k1::hashes::sha256;
use secp256k1::hashes::Hash;
use secp256k1::Message;
use spki::AlgorithmIdentifier;

use crate::pkg::SigningKeyDer;
use crate::sign::Error;

pub type PackageVerifier = VerifyingKey;
pub type PackageSigner = SigningKey;

pub struct SigningKey(pub(crate) secp256k1::SecretKey);

impl SigningKey {
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, Error> {
        Ok(Self(
            secp256k1::SecretKey::from_byte_array(bytes).map_err(|_| Error)?,
        ))
    }

    pub fn generate() -> (Self, PackageVerifier) {
        let (signing_key, verifying_key) = generate_keypair(&mut OsRng);
        (Self(signing_key), VerifyingKey(verifying_key))
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        secp256k1::PublicKey::from_secret_key_global(&self.0).into()
    }

    pub fn to_der(&self) -> Result<Vec<u8>, Error> {
        SigningKeyDer::new(self)
            .map_err(|_| Error)?
            .to_der()
            .map_err(|_| Error)
    }

    pub fn from_der(der: &[u8]) -> Result<Self, Error> {
        SigningKeyDer::from_der(der)
            .map_err(|_| Error)?
            .signing_key()
    }

    /// Sign file.
    pub fn sign(&self, message: &[u8]) -> Result<Signature, Error> {
        self.sign_data(blake2b(message).as_bytes())
    }

    /// Sign raw data.
    pub fn sign_data(&self, message: &[u8]) -> Result<Signature, Error> {
        let message = sha256::Hash::hash(message);
        let message = Message::from_digest(message.to_byte_array());
        Ok(self.0.sign_ecdsa(message).into())
    }
}

impl From<secp256k1::SecretKey> for SigningKey {
    fn from(other: secp256k1::SecretKey) -> Self {
        Self(other)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct VerifyingKey(pub(crate) secp256k1::PublicKey);

impl VerifyingKey {
    pub fn to_der(&self) -> Result<Vec<u8>, Error> {
        let info = SubjectPublicKeyInfo {
            algorithm: AlgorithmIdentifier {
                oid: ObjectIdentifier::from_arcs([1, 2, 840, 10045, 2, 1]).map_err(|_| Error)?,
                parameters: Some(
                    ObjectIdentifier::from_arcs([1, 3, 132, 0, 10]).map_err(|_| Error)?,
                ),
            },
            subject_public_key: BitString::new(0, self.0.serialize_uncompressed())
                .map_err(|_| Error)?,
        };
        info.to_der().map_err(|_| Error)
    }

    pub fn from_der(der: &[u8]) -> Result<Self, Error> {
        let info = SubjectPublicKeyInfo::<Any, BitString>::from_der(der).map_err(|_| Error)?;
        let bytes = info
            .subject_public_key
            .as_bytes()
            .ok_or(Error)?
            .try_into()
            .map_err(|_| Error)?;
        let verifying_key =
            secp256k1::PublicKey::from_byte_array_uncompressed(bytes).map_err(|_| Error)?;
        Ok(Self(verifying_key))
    }

    /// Verify signed file.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), Error> {
        self.verify_data(blake2b(message).as_bytes(), signature)
    }

    /// Verify raw data.
    pub fn verify_data(&self, message: &[u8], signature: &Signature) -> Result<(), Error> {
        let message = sha256::Hash::hash(message);
        let message = Message::from_digest(message.to_byte_array());
        signature.verify(&message, &self.0).map_err(|_| Error)?;
        Ok(())
    }
}

impl From<secp256k1::PublicKey> for VerifyingKey {
    fn from(other: secp256k1::PublicKey) -> Self {
        Self(other)
    }
}

pub struct Signature(pub(crate) secp256k1::ecdsa::Signature);

impl Signature {
    pub fn new(signature: secp256k1::ecdsa::Signature) -> Self {
        Self(signature)
    }

    pub fn from_der(der: &[u8]) -> Result<Self, Error> {
        let mut signature = secp256k1::ecdsa::Signature::from_der(der).map_err(|_| Error)?;
        // FreeBSD's `pkg key --sign` sometimes produces denormalized signatures.
        signature.normalize_s();
        Ok(Self(signature))
    }
}

impl From<secp256k1::ecdsa::Signature> for Signature {
    fn from(other: secp256k1::ecdsa::Signature) -> Self {
        Self(other)
    }
}

impl Deref for Signature {
    type Target = secp256k1::ecdsa::Signature;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::process::Command;
    use std::process::Stdio;

    use arbtest::arbtest;
    use command_error::ChildExt;
    use command_error::CommandExt;
    use tempfile::TempDir;

    use super::*;

    #[ignore = "Needs FreeBSD's `pkg`"]
    #[test]
    fn freebsd_pkg_key_public() {
        let (signing_key, verifying_key) = SigningKey::generate();
        let workdir = TempDir::new().unwrap();
        let private_key_file = workdir.path().join("private-key");
        fs_err::write(private_key_file.as_path(), signing_key.to_der().unwrap()).unwrap();
        assert!(Command::new("ls")
            .arg("-l")
            .arg(private_key_file.as_path())
            .status_checked()
            .unwrap()
            .success());
        assert!(Command::new("openssl")
            .arg("asn1parse")
            .arg("-inform")
            .arg("der")
            .arg("-i")
            .arg("-in")
            .arg(private_key_file.as_path())
            .status_checked()
            .unwrap()
            .success());
        let output = Command::new("pkg")
            .arg("key")
            .arg("--public")
            .arg("-t")
            .arg("ecdsa")
            .arg(private_key_file.as_path())
            .stdout(Stdio::piped())
            .output_checked()
            .unwrap();
        assert_eq!(verifying_key.to_der().unwrap(), output.stdout);
        let pkg_verifying_key = VerifyingKey::from_der(&output.stdout[..]).unwrap();
        assert_eq!(verifying_key, pkg_verifying_key);
    }

    #[ignore = "Needs FreeBSD's `pkg`"]
    #[test]
    fn freebsd_pkg_key_create() {
        let workdir = TempDir::new().unwrap();
        let private_key_file = workdir.path().join("private-key");
        let output = Command::new("pkg")
            .arg("key")
            .arg("--create")
            .arg("-t")
            .arg("ecdsa")
            .arg(private_key_file.as_path())
            .stdout(Stdio::piped())
            .output_checked()
            .unwrap();
        assert!(Command::new("ls")
            .arg("-l")
            .arg(private_key_file.as_path())
            .status_checked()
            .unwrap()
            .success());
        assert!(Command::new("openssl")
            .arg("asn1parse")
            .arg("-inform")
            .arg("der")
            .arg("-i")
            .arg("-in")
            .arg(private_key_file.as_path())
            .status_checked()
            .unwrap()
            .success());
        let verifying_key = VerifyingKey::from_der(&output.stdout[..]).unwrap();
        let signing_key =
            SigningKey::from_der(&fs_err::read(private_key_file.as_path()).unwrap()).unwrap();
        assert_eq!(verifying_key, signing_key.verifying_key())
    }

    #[ignore = "Needs FreeBSD's `pkg`"]
    #[test]
    fn freebsd_pkg_key_sign() {
        arbtest(|_u| {
            let (signing_key, verifying_key) = SigningKey::generate();
            let workdir = TempDir::new().unwrap();
            let private_key_file = workdir.path().join("private-key");
            fs_err::write(private_key_file.as_path(), signing_key.to_der().unwrap()).unwrap();
            let mut child = Command::new("pkg")
                .arg("key")
                .arg("--sign")
                .arg("-t")
                .arg("ecdsa")
                .arg(private_key_file.as_path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn_checked()
                .unwrap();
            let message = b"hello world";
            {
                let mut stdin = child.child_mut().stdin.take().unwrap();
                stdin.write_all(message).unwrap();
            }
            let output = child.output_checked().unwrap();
            let signature = output.stdout;
            let signature = Signature::from_der(&signature[..]).unwrap();
            verifying_key.verify_data(message, &signature).unwrap();
            Ok(())
        });
    }
}
