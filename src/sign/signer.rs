pub trait Signer {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error>;
}

pub trait Verifier {
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), Error>;

    fn verify_any<I, S>(&self, message: &[u8], signatures: I) -> Result<(), Error>
    where
        I: Iterator<Item = S>,
        S: AsRef<[u8]>,
    {
        let mut ret = false;
        for sig in signatures {
            ret |= self.verify(message, sig.as_ref()).is_ok();
        }
        if ret {
            Ok(())
        } else {
            Err(Error)
        }
    }
}

/// Opaque error.
#[derive(Debug)]
pub struct Error;

pub struct NoSigner;

impl Signer for NoSigner {
    fn sign(&self, _message: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(Vec::new())
    }
}

pub struct NoVerifier;

impl Verifier for NoVerifier {
    fn verify(&self, _message: &[u8], _signature: &[u8]) -> Result<(), Error> {
        Ok(())
    }
}
