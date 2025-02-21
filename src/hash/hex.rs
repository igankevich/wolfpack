use std::fmt::Display;
use std::fmt::Formatter;

pub struct Hex<'a>(pub &'a [u8]);

impl Display for Hex<'_> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for x in self.0.iter() {
            write!(f, "{:02x}", x)?;
        }
        Ok(())
    }
}

pub struct UpperHex<'a>(pub &'a [u8]);

impl Display for UpperHex<'_> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for x in self.0.iter() {
            write!(f, "{:02X}", x)?;
        }
        Ok(())
    }
}
