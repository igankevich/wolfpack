use crate::sign::Error;
use der::asn1::BitStringRef;
use der::asn1::Sequence;
use der::asn1::Utf8StringRef;
use der::DecodeValue;
use der::Encode;
use der::EncodeValue;
use der::Header;
use der::Length;
use der::Reader;
use der::Writer;
use secp256k1::constants::SECRET_KEY_SIZE;
use zeroize::ZeroizeOnDrop;

use crate::pkg::SigningKey;

pub struct SigningKeyDer<'a> {
    application: Utf8StringRef<'a>,
    version: u8,
    signer: Utf8StringRef<'a>,
    key_type: Utf8StringRef<'a>,
    public: bool,
    key: SecretData,
}

impl SigningKeyDer<'_> {
    pub fn signing_key(&self) -> Result<SigningKey, Error> {
        self.key.signing_key()
    }
}

impl SigningKeyDer<'_> {
    pub fn new(signing_key: &SigningKey) -> der::Result<Self> {
        Ok(Self {
            application: Utf8StringRef::new(PKG_APPLICATION)?,
            version: 1,
            signer: Utf8StringRef::new(PKG_SIGNER)?,
            key_type: Utf8StringRef::new(PKG_KEY_TYPE)?,
            public: false,
            key: SecretData::new(signing_key),
        })
    }
}

impl<'a> DecodeValue<'a> for SigningKeyDer<'a> {
    fn decode_value<R: Reader<'a>>(reader: &mut R, _header: Header) -> der::Result<Self> {
        Ok(Self {
            application: reader.decode()?,
            version: reader.decode()?,
            signer: reader.decode()?,
            key_type: reader.decode()?,
            public: reader.decode()?,
            key: {
                let bytes: BitStringRef = reader.decode()?;
                SecretData::from_bitstring_ref(bytes)
                    .ok_or_else(|| der::Error::new(der::ErrorKind::Failed, reader.position()))?
            },
        })
    }
}

impl EncodeValue for SigningKeyDer<'_> {
    fn value_len(&self) -> der::Result<Length> {
        self.application.encoded_len()?
            + self.version.encoded_len()?
            + self.signer.encoded_len()?
            + self.key_type.encoded_len()?
            + self.public.encoded_len()?
            + self.key.to_bistring_ref()?.encoded_len()?
    }

    fn encode_value(&self, writer: &mut impl Writer) -> ::der::Result<()> {
        self.application.encode(writer)?;
        self.version.encode(writer)?;
        self.signer.encode(writer)?;
        self.key_type.encode(writer)?;
        self.public.encode(writer)?;
        self.key.to_bistring_ref()?.encode(writer)?;
        Ok(())
    }
}

impl<'a> Sequence<'a> for SigningKeyDer<'a> {}

#[derive(ZeroizeOnDrop)]
struct SecretData {
    data: [u8; PKG_SECRET_DATA_SIZE],
}

impl SecretData {
    fn new(signing_key: &SigningKey) -> Self {
        let mut data = [0_u8; PKG_SECRET_DATA_SIZE];
        data[0] = PKG_PUBKEY_UNCOMPRESSED;
        let offset = PKG_SECRET_DATA_SIZE - signing_key.0[..].len();
        data[offset..].copy_from_slice(&signing_key.0[..]);
        Self { data }
    }

    fn to_bistring_ref(&self) -> der::Result<BitStringRef<'_>> {
        BitStringRef::new(0, &self.data[..])
    }

    fn from_bitstring_ref(data: BitStringRef<'_>) -> Option<Self> {
        Some(Self {
            data: data.as_bytes()?.try_into().ok()?,
        })
    }

    fn signing_key(&self) -> Result<SigningKey, Error> {
        Ok(secp256k1::SecretKey::from_byte_array(
            self.data[(self.data.len() - SECRET_KEY_SIZE)..]
                .try_into()
                .expect("The slice length is `SECRET_KEY_SIZE`"),
        )
        .map_err(|_| Error)?
        .into())
    }
}

// libecc format
const PKG_SECRET_DATA_SIZE: usize = 1 + 114;
// libecc format
const PKG_PUBKEY_UNCOMPRESSED: u8 = 4;

const PKG_APPLICATION: &str = "pkg";
const PKG_SIGNER: &str = "ecc";
const PKG_KEY_TYPE: &str = "SECP256K1";
