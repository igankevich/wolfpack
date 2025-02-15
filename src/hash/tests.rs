use std::fmt::Debug;
use std::fmt::Display;
use std::str::FromStr;

use arbitrary::Arbitrary;
use arbtest::arbtest;
use rand::rngs::OsRng;
use rand::RngCore;

use super::*;
use crate::hash::BUFFER_LEN;

pub fn same_as_computing_hash_of_the_whole_file<H: Hasher>()
where
    H::Output: PartialEq + Debug,
{
    arbtest(|u| {
        let spread = BUFFER_LEN / 2;
        let mut data = vec![0_u8; BUFFER_LEN - spread + u.int_in_range::<usize>(0..=2 * spread)?];
        OsRng.fill_bytes(&mut data);
        let (actual_hash, size) = HashingReader::<&[u8], H>::new(&data[..]).digest().unwrap();
        assert_eq!(data.len() as u64, size);
        let mut hasher = H::new();
        hasher.update(&data);
        let expected_hash = hasher.finalize();
        assert_eq!(expected_hash, actual_hash);
        Ok(())
    });
}

pub fn display_parse<T: Display + FromStr + Debug + PartialEq + for<'a> Arbitrary<'a>>() {
    arbtest(|u| {
        let expected: T = u.arbitrary()?;
        let string = expected.to_string();
        let actual: T = string
            .parse()
            .map_err(|_| panic!("string {:?}", string))
            .unwrap();
        assert_eq!(
            expected, actual,
            "expected = {:?}, actual = {:?}, string = {:?}",
            expected, actual, string
        );
        Ok(())
    });
}
