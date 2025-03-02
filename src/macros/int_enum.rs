macro_rules! define_int_enum {
    { $enum:ident, $int:ty, $(($name:ident, $value:ident),)* } => {
        #[derive(
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Debug,
        )]
        #[cfg_attr(test, derive(arbitrary::Arbitrary))]
        #[repr($int)]
        pub enum $enum {
            $( $name = $value, )*
        }

        impl $enum {
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $( $enum::$name => stringify!($value), )*
                }
            }
        }

        impl std::fmt::Display for $enum {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.as_str())
            }
        }

        impl From<$enum> for $int {
            fn from(other: $enum) -> $int {
                other as $int
            }
        }

        impl TryFrom<$int> for $enum {
            type Error = std::io::Error;
            fn try_from(other: $int) -> Result<Self, Self::Error> {
                match other {
                    $( $value => Ok($enum::$name), )*
                    _ => Err(std::io::ErrorKind::InvalidData.into()),
                }
            }
        }
    }
}

pub(crate) use define_int_enum;
