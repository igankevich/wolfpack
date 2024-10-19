use std::io::Error;

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum IndexEntryKind {
    Null = 0,
    Char = 1,
    Int8 = 2,
    Int16 = 3,
    Int32 = 4,
    Int64 = 5,
    String = 6,
    Bin = 7,
    StringArray = 8,
    I18nString = 9,
}

impl IndexEntryKind {
    pub fn validate_count(self, count: u32) -> Result<(), Error> {
        use IndexEntryKind::*;
        if count == 0 || matches!(self, String | I18nString if count != 1) {
            return Err(Error::other(format!("{:?}: invalid count", self)));
        }
        Ok(())
    }

    /*
    pub const fn len(self, count: usize) -> usize {
        // TODO overflow
        use IndexEntryKind::*;
        match self {
            Null => 0,
            Char => count,
            Int8 => count,
            Int16 => 2*count,
            Int32 => 4*count,
            Int64 => 8*count,
            String => 6*count,
            Bin => count,
            StringArray => count,
            I18nString => count,
        }
    }
    */
}

impl TryFrom<u32> for IndexEntryKind {
    type Error = Error;
    fn try_from(other: u32) -> Result<Self, Error> {
        use IndexEntryKind::*;
        match other {
            0 => Ok(Null),
            1 => Ok(Char),
            2 => Ok(Int8),
            3 => Ok(Int16),
            4 => Ok(Int32),
            5 => Ok(Int64),
            6 => Ok(String),
            7 => Ok(Bin),
            8 => Ok(StringArray),
            9 => Ok(I18nString),
            other => Err(Error::other(format!("invalid index entry kind: {}", other))),
        }
    }
}
