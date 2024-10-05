use std::time::SystemTime;

use pgp::crypto::{hash::HashAlgorithm, public_key::PublicKeyAlgorithm};
use pgp::packet::*;
use pgp::types::public::PublicParams;
use pgp::types::PublicKeyTrait;
use pgp::SignedPublicKey;
use pgp::SignedSecretKey;

use crate::sign::Error;
use crate::sign::Signer;
use crate::sign::Verifier;

pub struct PgpSigner {
    signing_key: SignedSecretKey,
    signature_type: SignatureType,
    hash_algorithm: HashAlgorithm,
}

impl PgpSigner {
    pub fn new(
        signing_key: SignedSecretKey,
        signature_type: SignatureType,
        hash_algorithm: HashAlgorithm,
    ) -> Self {
        Self {
            signing_key,
            signature_type,
            hash_algorithm,
        }
    }
}

impl Signer for PgpSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let mut config = SignatureConfig::v4(
            self.signature_type,
            get_public_key_algorithm(&self.signing_key)?,
            self.hash_algorithm,
        );
        config.unhashed_subpackets = vec![Subpacket::regular(SubpacketData::Issuer(
            self.signing_key.key_id(),
        ))];
        config.hashed_subpackets = vec![
            Subpacket::regular(SubpacketData::IssuerFingerprint(
                self.signing_key.fingerprint(),
            )),
            Subpacket::regular(SubpacketData::SignatureCreationTime(
                SystemTime::now().into(),
            )),
        ];
        let signature = config
            .sign(&self.signing_key, String::new, message)
            .map_err(|_| Error)?;
        let mut signature_bytes = Vec::with_capacity(1024);
        write_packet(&mut signature_bytes, &signature).map_err(|_| Error)?;
        Ok(signature_bytes)
    }
}

pub struct PgpVerifier {
    verifying_key: SignedPublicKey,
    no_signature_is_ok: bool,
}

impl PgpVerifier {
    pub fn new(verifying_key: SignedPublicKey) -> Self {
        Self {
            verifying_key,
            no_signature_is_ok: false,
        }
    }

    pub fn no_signature_is_ok(&mut self, value: bool) {
        self.no_signature_is_ok = value;
    }
}

impl Verifier for PgpVerifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Error> {
        let mut parser = PacketParser::new(signature);
        let packet = parser.next().ok_or(Error)?.map_err(|_| Error)?;
        if parser.next().is_some() {
            return Err(Error);
        }
        let signature = match packet {
            Packet::Signature(signature) => signature,
            _ => return Err(Error),
        };
        signature
            .verify(&self.verifying_key, message)
            .map_err(|_| Error)
    }

    fn verify_any<I, S>(&self, message: &[u8], signatures: I) -> Result<(), Error>
    where
        I: Iterator<Item = S>,
        S: AsRef<[u8]>,
    {
        let mut ret = false;
        let mut num_signatures: usize = 0;
        for sig in signatures {
            ret |= self.verify(message, sig.as_ref()).is_ok();
            num_signatures += 1;
        }
        if ret || (self.no_signature_is_ok && num_signatures == 0) {
            Ok(())
        } else {
            Err(Error)
        }
    }
}

fn get_public_key_algorithm<P: PublicKeyTrait>(
    public_key: &P,
) -> Result<PublicKeyAlgorithm, Error> {
    use PublicParams::*;
    match public_key.public_params() {
        RSA { .. } => Ok(PublicKeyAlgorithm::RSA),
        DSA { .. } => Ok(PublicKeyAlgorithm::DSA),
        ECDSA { .. } => Ok(PublicKeyAlgorithm::ECDSA),
        ECDH { .. } => Ok(PublicKeyAlgorithm::ECDH),
        Elgamal { .. } => Ok(PublicKeyAlgorithm::Elgamal),
        EdDSALegacy { .. } => Ok(PublicKeyAlgorithm::EdDSALegacy),
        Ed25519 { .. } => Ok(PublicKeyAlgorithm::Ed25519),
        X25519 { .. } => Ok(PublicKeyAlgorithm::X25519),
        X448 { .. } => Ok(PublicKeyAlgorithm::X448),
        Unknown { .. } => Err(Error),
    }
}

#[cfg(test)]
mod tests {
    use pgp::composed::*;

    use super::*;
    use crate::test::pgp_keys;

    #[test]
    fn sign_verify() {
        let message = "hello world";
        let (signing_key, verifying_key) = pgp_keys(KeyType::Ed25519);
        let signer = PgpSigner::new(signing_key, SignatureType::Binary, HashAlgorithm::SHA2_256);
        let signature = signer.sign(message.as_bytes()).unwrap();
        let verifier = PgpVerifier::new(verifying_key);
        verifier
            .verify(message.as_bytes(), signature.as_slice())
            .unwrap();
    }
}
