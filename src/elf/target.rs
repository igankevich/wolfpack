use std::collections::HashSet;
use std::io::Error;
use std::path::Path;

use elb::ArmFlags;
use elb::ByteOrder;
use elb::Class;
use elb::Elf;
use elb::Machine;
use fs_err::File;
use walkdir::WalkDir;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Target {
    pub flags: u32,
    pub machine: Machine,
    pub class: Class,
    pub byte_order: ByteOrder,
}

impl Target {
    pub fn read<P: AsRef<Path>>(file: P) -> Result<Self, elb::Error> {
        let mut file = File::open(file.as_ref())?;
        let elf = Elf::read(&mut file, 4096)?;
        Ok(Self {
            class: elf.header.class,
            byte_order: elf.header.byte_order,
            machine: elf.header.machine,
            flags: elf.header.flags,
        })
    }

    pub fn try_read<P: AsRef<Path>>(file: P) -> Result<Option<Self>, Error> {
        match Self::read(file) {
            Ok(target) => Ok(Some(target)),
            Err(elb::Error::Io(e)) => Err(e),
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
                let flags = ArmFlags::from_bits_truncate(self.flags);
                if flags.contains(ArmFlags::SOFT_FLOAT) {
                    "-sf"
                } else if flags.contains(ArmFlags::HARD_FLOAT) {
                    "-hf"
                } else {
                    ""
                }
            }
            _ => "",
        };
        write!(f, "{:?}-{}-{}{}", self.machine, bitness, byte_order, flags)
    }
}

pub(crate) mod macros {
    macro_rules! target {
        ($machine:ident) => {
            Some($crate::elf::Target {
                machine: ::elb::Machine::$machine,
                ..
            })
        };
        ($machine:ident, $byte_order:ident) => {
            Some($crate::elf::Target {
                machine: ::elb::Machine::$machine,
                byte_order: ::elb::ByteOrder::$byte_order,
                ..
            })
        };
        ($machine:ident, $byte_order:ident, $class:ident) => {
            Some($crate::elf::Target {
                machine: ::elb::Machine::$machine,
                byte_order: ::elb::ByteOrder::$byte_order,
                class: ::elb::Class::$class,
                ..
            })
        };
    }

    pub(crate) use target;
}
