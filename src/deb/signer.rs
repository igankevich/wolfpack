use std::time::SystemTime;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verify {
    Always,
    Never,
    OnlyIfPresent,
}

pub struct PackageVerifier {
    verifying_keys: Vec<VerifyingKey>,
    verify: Verify,
}

impl PackageVerifier {
    pub fn new(verifying_keys: Vec<VerifyingKey>, verify: Verify) -> Self {
        Self {
            verifying_keys,
            verify,
        }
    }

    pub fn none() -> Self {
        Self {
            verifying_keys: Default::default(),
            verify: Verify::Never,
        }
    }
}

impl Verifier for PackageVerifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Error> {
        if self.verify == Verify::Never {
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
        if self.verify == Verify::Never {
            return Ok(());
        }
        let mut has_signatures = false;
        for sig in signatures {
            has_signatures = true;
            if self.verify(message, sig.as_ref()).is_ok() {
                return Ok(());
            }
        }
        if !has_signatures && self.verify == Verify::OnlyIfPresent {
            return Ok(());
        }
        Err(Error)
    }
}

#[derive(Clone)]
pub struct SigningKey(SignedSecretKey);

impl SigningKey {
    pub fn to_armored_string(&self) -> Result<String, Error> {
        self.0
            .to_armored_string(Default::default())
            .map_err(|_| Error)
    }

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

    pub fn from_bytes(
        bytes: &[u8; ed25519_dalek::SECRET_KEY_LENGTH],
    ) -> Result<(SigningKey, VerifyingKey), Error> {
        use pgp::crypto::ecc_curve::ECCCurve;
        use pgp::packet;
        use pgp::packet::KeyFlags;
        use pgp::packet::UserId;
        use pgp::types::Mpi;
        use pgp::types::PlainSecretParams;
        use pgp::types::PublicParams;
        use pgp::types::SecretParams;
        use pgp::KeyDetails;
        use pgp::SecretKey;
        // First repeat the code from `pgp::crypto::eddsa::generate_key`, but
        // use the provided byte slice.
        let secret = ed25519_dalek::SigningKey::from_bytes(bytes);
        let public = ed25519_dalek::VerifyingKey::from(&secret);
        // public key
        let mut q = Vec::with_capacity(ed25519_dalek::PUBLIC_KEY_LENGTH + 1);
        q.push(0x40);
        q.extend_from_slice(&public.to_bytes());
        // secret key
        let p = Mpi::from_slice(&secret.to_bytes());
        let public_params = PublicParams::EdDSALegacy {
            curve: ECCCurve::Ed25519,
            q: Mpi::from_raw(q),
        };
        let secret_params = SecretParams::Plain(PlainSecretParams::EdDSALegacy(p));
        // Now, repeat the code from `SecretKeyParams::generate` with our
        // public and secret params.
        let packet_version = Default::default();
        let version = Default::default();
        let key_type = KeyType::EdDSALegacy;
        let created_at = SystemTime::UNIX_EPOCH;
        let expiration = None;
        let primary_key = packet::SecretKey::new(
            packet::PublicKey::new(
                packet_version,
                version,
                key_type.to_alg(),
                created_at.into(),
                expiration,
                public_params,
            )
            .map_err(|_| Error)?,
            secret_params,
        );
        let key_flags = {
            let mut flags = KeyFlags::default();
            flags.set_certify(false);
            flags.set_encrypt_comms(false);
            flags.set_encrypt_storage(false);
            flags.set_sign(true);
            flags
        };
        let secret_key = SecretKey::new(
            primary_key,
            KeyDetails::new(
                UserId::from_str(Default::default(), USER_ID),
                Default::default(),
                Default::default(),
                key_flags,
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                None,
            ),
            Default::default(),
            Default::default(),
        );
        // OsRng is unused in these functions for Ed25519 algorithm.
        let signed_secret_key = secret_key.sign(OsRng, String::new).map_err(|_| Error)?;
        let signed_public_key = signed_secret_key
            .public_key()
            .sign(OsRng, &signed_secret_key, String::new)
            .map_err(|_| Error)?;
        Ok((SigningKey(signed_secret_key), signed_public_key.into()))
    }
}

impl From<SigningKey> for SignedSecretKey {
    fn from(other: SigningKey) -> Self {
        other.0
    }
}

const USER_ID: &str = "wolfpack-pgp";
