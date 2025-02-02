use arbitrary::Arbitrary;
use arbtest::arbtest;
use std::any::type_name;
use std::fmt::Debug;
use std::str::FromStr;

pub fn to_string_parse_symmetry<T>()
where
    T: FromStr + ToString + Debug + PartialEq + Eq + for<'a> Arbitrary<'a>,
    <T as FromStr>::Err: Debug,
{
    to_string_parse_symmetry_from::<T, T>()
}

pub fn to_string_parse_symmetry_from<X, T>()
where
    X: for<'a> Arbitrary<'a>,
    T: FromStr + ToString + Debug + PartialEq + Eq + From<X>,
    <T as FromStr>::Err: Debug,
{
    arbtest(|u| {
        let expected: X = u.arbitrary()?;
        let expected: T = expected.into();
        let string = expected.to_string();
        let actual: T = string
            .parse()
            .inspect_err(|e| {
                panic!(
                    "Failed to parse `{}` as `{}`: {:?}",
                    string,
                    type_name::<T>(),
                    e
                )
            })
            .unwrap();
        similar_asserts::assert_eq!(
            expected,
            actual,
            "expected = {expected:?}, actual = {actual:?}, string = {string:?}"
        );
        Ok(())
    });
}
