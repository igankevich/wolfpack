use std::ffi::CStr;
use std::ffi::CString;
use std::io::Error;
use std::io::Write;
use std::str::from_utf8;

use crate::hash::Md5Hash;
use crate::hash::Sha1Hash;
use crate::hash::Sha256Hash;

pub trait ValueIo {
    fn read(input: &[u8], count: usize) -> Result<Self, Error>
    where
        Self: Sized;

    fn write<W: Write>(&self, writer: W) -> Result<(), Error>;

    fn count(&self) -> usize {
        1
    }
}

impl ValueIo for u8 {
    fn read(input: &[u8], _count: usize) -> Result<u8, Error> {
        Ok(input[0])
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(&[*self])
    }
}

impl ValueIo for u16 {
    fn read(input: &[u8], _count: usize) -> Result<u16, Error> {
        Ok(u16::from_be_bytes([input[0], input[1]]))
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(self.to_be_bytes().as_slice())
    }
}

impl ValueIo for u32 {
    fn read(input: &[u8], _count: usize) -> Result<u32, Error> {
        Ok(u32::from_be_bytes([input[0], input[1], input[2], input[3]]))
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(self.to_be_bytes().as_slice())
    }
}

impl ValueIo for u64 {
    fn read(input: &[u8], _count: usize) -> Result<u64, Error> {
        Ok(u64::from_be_bytes([
            input[0], input[1], input[2], input[3], input[4], input[5], input[6], input[7],
        ]))
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(self.to_be_bytes().as_slice())
    }
}

value_io_array!(u16, 2);
value_io_array!(u32, 4);
value_io_array!(u64, 8);

impl ValueIo for Vec<u8> {
    fn read(input: &[u8], count: usize) -> Result<Vec<u8>, Error> {
        Ok(input[..count].into())
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(self)
    }

    fn count(&self) -> usize {
        self.len()
    }
}

impl ValueIo for String {
    fn read(input: &[u8], _count: usize) -> Result<String, Error> {
        let c_str = CStr::from_bytes_until_nul(input)
            .map_err(|_| Error::other("string is not terminated"))?;
        let s = from_utf8(c_str.to_bytes())
            .map_err(|e| Error::other(format!("invalid utf-8 string: {}", e)))?;
        Ok(s.into())
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(self.as_bytes())?;
        writer.write_all(&[0_u8])?;
        Ok(())
    }
}

impl ValueIo for CString {
    fn read(input: &[u8], _count: usize) -> Result<Self, Error> {
        let c_str = CStr::from_bytes_until_nul(input)
            .map_err(|_| Error::other("string is not terminated"))?;
        Ok(c_str.into())
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(self.as_bytes_with_nul())?;
        Ok(())
    }
}

impl ValueIo for Vec<CString> {
    fn read(mut input: &[u8], count: usize) -> Result<Self, Error> {
        let mut strings = Vec::with_capacity(count);
        for _ in 0..count {
            let c_string = CString::read(input, 1)?;
            let n = c_string.as_bytes_with_nul().len();
            strings.push(c_string);
            input = &input[n..];
        }
        Ok(strings)
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        for s in self {
            s.write(writer.by_ref())?;
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.len()
    }
}

impl ValueIo for Vec<String> {
    fn read(mut input: &[u8], count: usize) -> Result<Vec<String>, Error> {
        let mut strings = Vec::with_capacity(count);
        for _ in 0..count {
            let s = String::read(input, 1)?;
            let n = s.len();
            strings.push(s);
            input = &input[(n + 1)..];
        }
        Ok(strings)
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        for s in self {
            writer.write_all(s.as_bytes())?;
            writer.write_all(&[0_u8])?;
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.len()
    }
}

impl ValueIo for Md5Hash {
    fn read(input: &[u8], count: usize) -> Result<Md5Hash, Error> {
        input
            .get(..count)
            .ok_or_else(|| Error::other("invalid md5 size"))?
            .try_into()
            .map_err(|_| Error::other("invalid md5 size"))
    }

    fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
        writer.write_all(self.as_slice())
    }
}

impl ValueIo for Sha1Hash {
    fn read(input: &[u8], count: usize) -> Result<Sha1Hash, Error> {
        String::read(input, count)?
            .parse()
            .map_err(|_| Error::other("invalid sha1"))
    }

    fn write<W: Write>(&self, writer: W) -> Result<(), Error> {
        self.to_string().write(writer)
    }
}

impl ValueIo for Sha256Hash {
    fn read(input: &[u8], count: usize) -> Result<Sha256Hash, Error> {
        String::read(input, count)?
            .parse()
            .map_err(|_| Error::other("invalid sha256"))
    }

    fn write<W: Write>(&self, writer: W) -> Result<(), Error> {
        self.to_string().write(writer)
    }
}

macro_rules! value_io_array {
    ($type:ty, $size:literal) => {
        impl ValueIo for Vec<$type> {
            fn read(mut input: &[u8], count: usize) -> Result<Vec<$type>, Error> {
                let mut array = Vec::with_capacity(count);
                for _ in 0..count {
                    array.push(<$type as ValueIo>::read(input, 1)?);
                    input = &input[$size..];
                }
                Ok(array)
            }

            fn write<W: Write>(&self, mut writer: W) -> Result<(), Error> {
                for n in self {
                    <$type as ValueIo>::write(n, writer.by_ref())?;
                }
                Ok(())
            }

            fn count(&self) -> usize {
                self.len()
            }
        }
    };
}

use value_io_array;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpm::test::write_read_symmetry;

    #[test]
    fn symmetry() {
        write_read_symmetry::<u8>();
        write_read_symmetry::<u16>();
        write_read_symmetry::<u32>();
        write_read_symmetry::<u64>();
        write_read_symmetry::<CString>();
        write_read_symmetry::<Vec<u8>>();
        write_read_symmetry::<Vec<u16>>();
        write_read_symmetry::<Vec<u32>>();
        write_read_symmetry::<Vec<u64>>();
        write_read_symmetry::<Vec<CString>>();
    }
}
