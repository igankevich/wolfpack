use std::collections::HashSet;
use std::io::Error;
use std::io::Read;
use std::path::Path;

use elf::abi::EI_NIDENT;
use elf::endian::AnyEndian;
use elf::file::FileHeader;
use elf::parse::ParseError;
use fs_err::File;
use walkdir::WalkDir;

use crate::elf::ByteOrder;
use crate::elf::Class;
use crate::elf::Flags;
use crate::elf::Machine;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Target {
    pub flags: Flags,
    pub machine: Machine,
    pub class: Class,
    pub byte_order: ByteOrder,
}

impl Target {
    pub fn read<P: AsRef<Path>>(file: P) -> Result<Self, ParseError> {
        let mut file = File::open(file.as_ref()).map_err(ParseError::IOError)?;
        let mut buf = [0; 64];
        let n = file.read(&mut buf[..]).map_err(ParseError::IOError)?;
        let buf = &buf[..n];
        drop(file);
        // `elf` crate panics on small buffers.
        if buf.len() < 4 {
            return Err(elf::ParseError::BadOffset(4));
        }
        let ident = elf::file::parse_ident::<AnyEndian>(buf)?;
        let header = FileHeader::<AnyEndian>::parse_tail(ident, &buf[EI_NIDENT..])?;
        Ok(Self {
            class: header.class.into(),
            byte_order: header.endianness.into(),
            machine: header.e_machine.try_into()?,
            flags: Flags::from_bits_truncate(header.e_flags),
        })
    }

    pub fn try_read<P: AsRef<Path>>(file: P) -> Result<Option<Self>, Error> {
        match Self::read(file) {
            Ok(target) => Ok(Some(target)),
            Err(ParseError::IOError(e)) => Err(e),
            Err(_) => Ok(None),
        }
    }

    pub fn scan_dir<P: AsRef<Path>>(dir: P) -> Result<HashSet<Self>, Error> {
        let mut targets = HashSet::new();
        for entry in WalkDir::new(dir).into_iter() {
            let entry = entry?;
            if let Some(target) = Self::try_read(entry.path())? {
                targets.insert(target);
            }
        }
        Ok(targets)
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let bitness = match self.class {
            Class::Elf32 => "32",
            Class::Elf64 => "64",
        };
        let byte_order = match self.byte_order {
            ByteOrder::LittleEndian => "le",
            ByteOrder::BigEndian => "be",
        };
        let flags = match self.machine {
            Machine::Arm => {
                if self.flags.contains(Flags::ARM_SOFT_FLOAT) {
                    "-sf"
                } else if self.flags.contains(Flags::ARM_HARD_FLOAT) {
                    "-hf"
                } else {
                    ""
                }
            }
            _ => "",
        };
        write!(f, "{}-{}-{}{}", self.machine, bitness, byte_order, flags)
    }
}

pub(crate) mod macros {
    macro_rules! target {
        ($machine:ident) => {
            Some(crate::elf::Target {
                machine: crate::elf::Machine::$machine,
                ..
            })
        };
        ($machine:ident, $byte_order:ident) => {
            Some(crate::elf::Target {
                machine: crate::elf::Machine::$machine,
                byte_order: crate::elf::ByteOrder::$byte_order,
                ..
            })
        };
        ($machine:ident, $byte_order:ident, $class:ident) => {
            Some(crate::elf::Target {
                machine: crate::elf::Machine::$machine,
                byte_order: crate::elf::ByteOrder::$byte_order,
                class: crate::elf::Class::$class,
                ..
            })
        };
    }

    pub(crate) use target;
}
