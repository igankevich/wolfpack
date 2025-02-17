macro_rules! define_arch_enum {
    { $enum:ident, $(($name:ident, $str:literal),)* } => {
        #[derive(
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Debug,
            serde::Serialize,
            serde::Deserialize
        )]
        #[cfg_attr(test, derive(arbitrary::Arbitrary))]
        #[serde(into = "String", try_from = "String")]
        pub enum $enum {
            $( $name, )*
        }

        impl $enum {
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $( $enum::$name => $str, )*
                }
            }
        }

        impl std::fmt::Display for $enum {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.as_str())
            }
        }

        impl std::str::FromStr for $enum {
            type Err = std::io::Error;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $( $str => Ok($enum::$name), )*
                    _ => Err(std::io::ErrorKind::InvalidData.into()),
                }
            }
        }

        impl From<$enum> for String {
            fn from(other: $enum) -> String {
                other.as_str().to_string()
            }
        }

        impl TryFrom<String> for $enum {
            type Error = std::io::Error;
            fn try_from(other: String) -> Result<Self, Self::Error> {
                other.parse()
            }
        }
    }
}

pub(crate) use define_arch_enum;

#[allow(unused)]
macro_rules! define_arch_try_from {
    { $from:ty, $to:ty, $(($name1:ident, $name2:ident),)* } => {
        impl TryFrom<$from> for $to {
            type Error = std::io::Error;

            fn try_from(other: $from) -> Result<Self, Self::Error> {
                match other {
                    $( <$from>::$name1 => Ok(<$to>::$name2), )*
                    _ => Err(std::io::ErrorKind::InvalidData.into()),
                }
            }
        }
    }
}

#[allow(unused)]
pub(crate) use define_arch_try_from;

macro_rules! define_arch_from {
    { $from:ty, $to:ty, $(($name1:ident, $name2:ident),)* } => {
        impl From<$from> for $to {
            fn from(other: $from) -> Self {
                match other {
                    $( <$from>::$name1 => <$to>::$name2, )*
                }
            }
        }
    }
}

pub(crate) use define_arch_from;
