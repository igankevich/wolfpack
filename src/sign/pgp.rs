use std::io::Read;
use std::io::Write;
use std::ops::Deref;
use std::time::SystemTime;

use pgp::cleartext::CleartextSignedMessage;
use pgp::composed::signed_key::SignedPublicKeyParser;
use pgp::composed::StandaloneSignature;
use pgp::crypto::{hash::HashAlgorithm, public_key::PublicKeyAlgorithm};
use pgp::packet::*;
use pgp::types::public::PublicParams;
use pgp::types::PublicKeyTrait;
use pgp::Deserializable;
use pgp::SignedPublicKey;
use pgp::SignedSecretKey;
use rand::rngs::OsRng;

use crate::sign::Error;
use crate::sign::Signer;
use crate::sign::Verifier;
use crate::sign::VerifierV2;

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

    pub fn sign_v2(&self, message: &[u8]) -> Result<PgpSignature, Error> {
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
        Ok(PgpSignature(signature))
    }
}

impl Signer for PgpSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let signature = self.sign_v2(message)?;
        let mut signature_bytes = Vec::with_capacity(1024);
        signature
            .write_binary(&mut signature_bytes)
            .map_err(|_| Error)?;
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

#[derive(Clone, Debug)]
pub struct PgpVerifyingKey(SignedPublicKey);

impl PgpVerifyingKey {
    pub fn write_armored<W: Write>(&self, mut writer: W) -> Result<(), std::io::Error> {
        self.0
            .to_armored_writer(writer.by_ref(), Default::default())
            .map_err(std::io::Error::other)
    }

    pub fn read_armored_one<R: Read>(reader: R) -> Result<Self, std::io::Error> {
        let (public_key, _headers) =
            SignedPublicKey::from_reader_single(reader).map_err(std::io::Error::other)?;
        Ok(Self(public_key))
    }

    pub fn read_binary_all<R: Read>(reader: R) -> Result<Vec<Self>, std::io::Error> {
        let mut public_keys = Vec::new();
        let parser = PacketParser::new(reader);
        let parser = SignedPublicKeyParser::from_packets(parser.peekable());
        for packet in parser {
            let packet = packet.map_err(std::io::Error::other)?;
            public_keys.push(Self(packet));
        }
        Ok(public_keys)
    }
}

impl VerifierV2 for PgpVerifyingKey {
    type Signature = PgpSignature;

    fn verify_v2(&self, message: &[u8], signature: &Self::Signature) -> Result<(), Error> {
        if self.0.is_signing_key() && signature.0.verify(&self.0, message).is_ok() {
            return Ok(());
        }
        for public_subkey in self
            .0
            .public_subkeys
            .iter()
            .filter(|pk| pk.is_signing_key())
        {
            if signature.0.verify(public_subkey, message).is_ok() {
                return Ok(());
            }
        }
        Err(Error)
    }
}

impl From<SignedPublicKey> for PgpVerifyingKey {
    fn from(other: SignedPublicKey) -> Self {
        Self(other)
    }
}

impl From<PgpVerifyingKey> for SignedPublicKey {
    fn from(other: PgpVerifyingKey) -> Self {
        other.0
    }
}

impl Deref for PgpVerifyingKey {
    type Target = SignedPublicKey;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct PgpSignature(Signature);

impl PgpSignature {
    pub fn to_binary(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buf = Vec::new();
        self.write_binary(&mut buf)?;
        Ok(buf)
    }

    pub fn write_binary<W: Write>(&self, mut writer: W) -> Result<(), std::io::Error> {
        write_packet(writer.by_ref(), &self.0).map_err(std::io::Error::other)
    }

    pub fn write_armored<W: Write>(&self, mut writer: W) -> Result<(), std::io::Error> {
        let signature = StandaloneSignature::new(self.0.clone());
        signature
            .to_armored_writer(writer.by_ref(), Default::default())
            .map_err(std::io::Error::other)
    }

    pub fn read_armored_one<R: Read>(reader: R) -> Result<Self, std::io::Error> {
        let (signature, _headers) =
            StandaloneSignature::from_reader_single(reader).map_err(std::io::Error::other)?;
        Ok(Self(signature.signature))
    }

    pub fn read_binary_all<R: Read>(reader: R) -> Result<Vec<Self>, std::io::Error> {
        let mut signatures = Vec::new();
        let parser = PacketParser::new(reader);
        for packet in parser {
            eprintln!("packet {:?}", packet);
            let packet = packet.map_err(std::io::Error::other)?;
            let signature = match packet {
                Packet::Signature(signature) => signature,
                // skip other packets
                _ => continue,
            };
            signatures.push(Self(signature));
        }
        Ok(signatures)
    }

    pub fn into_inner(self) -> Signature {
        self.0
    }
}

pub struct PgpCleartextSigner {
    signing_key: SignedSecretKey,
}

impl PgpCleartextSigner {
    pub fn new(signing_key: SignedSecretKey) -> Self {
        Self { signing_key }
    }

    pub fn sign(&self, message: &str) -> Result<CleartextSignedMessage, Error> {
        let signed_message =
            CleartextSignedMessage::sign(OsRng, message, &self.signing_key, String::new)
                .map_err(|_| Error)?;
        Ok(signed_message)
    }
}

pub struct PgpCleartextVerifier {
    verifying_key: SignedPublicKey,
}

impl PgpCleartextVerifier {
    pub fn new(verifying_key: SignedPublicKey) -> Self {
        Self { verifying_key }
    }

    pub fn verify(&self, signed_message: &CleartextSignedMessage) -> Result<(), Error> {
        signed_message
            .verify(&self.verifying_key)
            .map_err(|_| Error)?;
        Ok(())
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

    #[test]
    fn cleartext_sign_verify() {
        //let body = std::fs::read("InRelease.tmp").unwrap();
        //let body = std::fs::read("clearsign.txt").unwrap();
        //let data = pgp::composed::Any::from_armor(&body[..]).unwrap();
        //let data = pgp::composed::message::Message::from_armor_single(&body[..]).unwrap();
        //let data = CleartextSignedMessage::from_armor(&body[..]).unwrap();
        //eprintln!("{data:?}");
        //let data = pgp::packet::LiteralData::from_slice(Default::default(), &body[..]).unwrap();
        //eprintln!("{data:?}");
        //let mut parser = PacketParser::new(&body[..]);
        //for packet in parser {
        //    eprintln!("package {packet:?}");
        //}
        let message = "hello world";
        let (signing_key, verifying_key) = pgp_keys(KeyType::Ed25519);
        let signer = PgpCleartextSigner::new(signing_key);
        let signed_message = signer.sign(message).unwrap();
        let mut buf = Vec::new();
        signed_message
            .to_armored_writer(&mut buf, Default::default())
            .unwrap();
        let (signed_message, _headers) = CleartextSignedMessage::from_armor(&buf[..]).unwrap();
        let verifier = PgpCleartextVerifier::new(verifying_key);
        verifier.verify(&signed_message).unwrap();
    }
}
