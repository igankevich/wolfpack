macro_rules! define_try_from_string_from_string {
    ($type:ident) => {
        impl From<$type> for String {
            fn from(other: $type) -> String {
                other.to_string()
            }
        }

        impl TryFrom<String> for $type {
            type Error = <$type as FromStr>::Err;
            fn try_from(other: String) -> Result<Self, Self::Error> {
                other.parse()
            }
        }
    };
}

pub(crate) use define_try_from_string_from_string;
