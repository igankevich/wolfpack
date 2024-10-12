use std::ops::RangeInclusive;

pub const CONTROL: [char; 65] = [
    '\u{0000}', '\u{0001}', '\u{0002}', '\u{0003}', '\u{0004}', '\u{0005}', '\u{0006}', '\u{0007}',
    '\u{0008}', '\u{0009}', '\u{000A}', '\u{000B}', '\u{000C}', '\u{000D}', '\u{000E}', '\u{000F}',
    '\u{0010}', '\u{0011}', '\u{0012}', '\u{0013}', '\u{0014}', '\u{0015}', '\u{0016}', '\u{0017}',
    '\u{0018}', '\u{0019}', '\u{001A}', '\u{001B}', '\u{001C}', '\u{001D}', '\u{001E}', '\u{001F}',
    '\u{007F}', '\u{0080}', '\u{0081}', '\u{0082}', '\u{0083}', '\u{0084}', '\u{0085}', '\u{0086}',
    '\u{0087}', '\u{0088}', '\u{0089}', '\u{008A}', '\u{008B}', '\u{008C}', '\u{008D}', '\u{008E}',
    '\u{008F}', '\u{0090}', '\u{0091}', '\u{0092}', '\u{0093}', '\u{0094}', '\u{0095}', '\u{0096}',
    '\u{0097}', '\u{0098}', '\u{0099}', '\u{009A}', '\u{009B}', '\u{009C}', '\u{009D}', '\u{009E}',
    '\u{009F}',
];

// TODO ranges allow constructing invalid chars
pub const UNICODE: [RangeInclusive<char>; 2] = ['\u{0}'..='\u{d7ff}', '\u{e000}'..='\u{10FFFF}'];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_range() {
        for range in UNICODE {
            let (a, b) = range.into_inner();
            let a = a as u32;
            let b = b as u32;
            for i in a..=b {
                if i == 0xd8d7 {
                    eprintln!("ch {:?}", i);
                }
                if char::from_u32(i).is_none() {
                    eprintln!("failed on {:x}", i);
                }
            }
        }
    }
}
