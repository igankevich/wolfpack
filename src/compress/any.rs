use std::io::BufRead;
use std::io::BufReader;
use std::io::Error;
use std::io::Read;

use flate2::read::GzDecoder;
use xz::read::XzDecoder;
use zstd::stream::read::Decoder as ZstdDecoder;

pub struct AnyDecoder<'a, R: 'a + Read> {
    reader: Option<BufReader<R>>,
    decoder: Box<dyn Read + 'a>,
}

impl<'a, R: 'a + Read> AnyDecoder<'a, R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: Some(BufReader::new(reader)),
            decoder: Box::new(DummyDecoder),
        }
    }
}

impl<'a, R: 'a + Read> Read for AnyDecoder<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        if let Some(reader) = self.reader.take() {
            self.decoder = new_decoder(reader)?;
        }
        self.decoder.read(buf)
    }
}

fn new_decoder<'a, R: 'a + Read + BufRead>(mut reader: R) -> Result<Box<dyn Read + 'a>, Error> {
    let data = reader.fill_buf()?;
    match data {
        // https://tukaani.org/xz/xz-file-format-1.0.4.txt
        [0xfd, b'7', b'z', b'X', b'Z', 0, ..] => Ok(Box::new(XzDecoder::new(reader))),
        // RFC8878
        [0x28, 0xb5, 0x2f, 0xfd, ..] => Ok(Box::new(ZstdDecoder::new(reader)?)),
        // RFC1952
        [0x1f, 0x8b, 0x08, ..] => Ok(Box::new(GzDecoder::new(reader))),
        magic => Err(Error::other(format!(
            "unknown compression format: starting bytes {:?}",
            &magic[..MAX_BYTES.min(magic.len())],
        ))),
    }
}

const MAX_BYTES: usize = 6;

struct DummyDecoder;

impl Read for DummyDecoder {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }
}
