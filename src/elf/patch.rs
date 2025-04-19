use std::collections::BTreeSet;
use std::ffi::CString;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use elb::ByteOrder;
use elb::Class;
use elb::DynamicTag;
use elb::Elf;
use elb::ElfPatcher;
use elb::Machine;
use fs_err::File;
use fs_err::OpenOptions;
use lddtree::DependencyAnalyzer;

pub fn patch<P1, P2>(file: P1, rpath: P2, interpreter: Option<&Path>) -> Result<(), elb::Error>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    let c_rpath = {
        let mut bytes = rpath.as_ref().to_path_buf().into_os_string().into_vec();
        bytes.push(0_u8);
        unsafe { CString::from_vec_with_nul_unchecked(bytes) }
    };
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(file.as_ref())?;
    let elf = Elf::read(&mut file, page_size::get() as u64)?;
    let mut patcher = ElfPatcher::new(elf, file);
    if let Some(interpreter) = interpreter {
        let mut bytes = interpreter.to_path_buf().into_os_string().into_vec();
        bytes.push(0_u8);
        let c_interpreter = unsafe { CString::from_vec_with_nul_unchecked(bytes) };
        patcher.set_interpreter(c_interpreter.as_c_str())?;
    }
    patcher.set_library_search_path(DynamicTag::Runpath, c_rpath.as_c_str())?;
    patcher.finish()?;
    Ok(())
}

pub fn change_root<P1, P2>(file: P1, root: P2) -> Result<(), std::io::Error>
where
    P1: AsRef<Path>,
    P2: Into<PathBuf>,
{
    let root = root.into();
    let file = file.as_ref();
    log::info!("Changing root to {}: {}", root.display(), file.display());
    // TODO
    let analyzer = DependencyAnalyzer::new(root.clone())
        .add_library_path(root.join("lib/x86_64-linux-gnu"))
        .add_library_path(root.join("usr/lib/x86_64-linux-gnu"));
    let dependencies = match analyzer.analyze(file) {
        Ok(dependencies) => dependencies,
        Err(e) => {
            log::warn!("Failed to analyze dependencies of {:?}: {e}", file);
            return Ok(());
        }
    };
    // Change interpreter.
    let interpreter = if let Some(interpreter) = dependencies.interpreter.as_ref() {
        let library = dependencies.libraries.get(interpreter).ok_or_else(|| {
            std::io::Error::other(format!("Interpreter {} not found", interpreter))
        })?;
        let dest_file = match library.realpath.as_ref() {
            Some(realpath) => realpath.clone(),
            None => {
                if library.path.starts_with(&root) {
                    library.path.clone()
                } else {
                    Path::new(&root).join(&library.path)
                }
            }
        };
        let new_interpreter = canonicalize_in_root(&dest_file, &root)?;
        eprintln!("Set interp {:?} library {:?}", new_interpreter, library);
        Some(new_interpreter)
    } else {
        None
    };
    // TODO modify existing rpath
    // Change rpath.
    let mut rpath = BTreeSet::new();
    for (_file_name, library) in dependencies.libraries.iter() {
        let dest_file = match library.realpath.as_ref() {
            Some(realpath) => realpath.clone(),
            None => {
                let path = library
                    .path
                    .strip_prefix(&root)
                    .unwrap_or(library.path.as_path());
                Path::new(&root).join(path)
            }
        };
        rpath.insert(dest_file.parent().expect("Parent exists").to_path_buf());
    }
    // Join rpath with ":".
    let mut rpath_str = OsString::new();
    {
        let mut iter = rpath.into_iter();
        if let Some(dir) = iter.next() {
            rpath_str.push(dir);
        }
        for dir in iter {
            rpath_str.push(":");
            rpath_str.push(dir);
        }
    }
    eprintln!("Set rpath {:?}", rpath_str);
    patch(file, rpath_str, interpreter.as_deref()).map_err(std::io::Error::other)?;
    Ok(())
}

fn canonicalize_in_root(path: &Path, root: &Path) -> Result<PathBuf, std::io::Error> {
    use Component::*;
    let mut new_path = PathBuf::new();
    for comp in path.components() {
        new_path.push(comp);
        if !matches!(comp, Normal(..)) {
            continue;
        }
        let meta = fs_err::symlink_metadata(&new_path)?;
        if meta.is_symlink() {
            let target = fs_err::read_link(&new_path)?;
            if let Ok(rest) = target.strip_prefix("/") {
                // Replace symbolic links that use absolute paths.
                new_path = root.join(rest);
            }
        }
    }
    Ok(new_path)
}

pub fn is_native_elf<P: AsRef<Path>>(path: &P) -> Result<bool, elb::Error> {
    let Some(host_class) = HOST_CLASS else {
        return Ok(false);
    };
    let Some(host_byte_order) = HOST_BYTE_ORDER else {
        return Ok(false);
    };
    let Some(host_machine) = HOST_MACHINE else {
        return Ok(false);
    };
    let mut file = File::open(path.as_ref())?;
    let header = match elb::Header::read(&mut file) {
        Ok(header) => header,
        Err(elb::Error::NotElf) => return Ok(false),
        Err(e) => return Err(e),
    };
    Ok(header.byte_order == host_byte_order
        && header.class == host_class
        && header.machine == host_machine)
}

const HOST_BYTE_ORDER: Option<ByteOrder> = if cfg!(target_endian = "little") {
    Some(ByteOrder::LittleEndian)
} else if cfg!(target_endian = "big") {
    Some(ByteOrder::BigEndian)
} else {
    None
};

const HOST_CLASS: Option<Class> = if cfg!(target_pointer_width = "32") {
    Some(Class::Elf32)
} else if cfg!(target_pointer_width = "64") {
    Some(Class::Elf64)
} else {
    None
};

const HOST_MACHINE: Option<Machine> = if cfg!(target_arch = "x86_64") {
    Some(Machine::X86_64)
} else if cfg!(target_arch = "arm") {
    Some(Machine::Arm)
} else if cfg!(target_arch = "aarch64") {
    Some(Machine::Aarch64)
} else if cfg!(target_arch = "mips") {
    Some(Machine::Mips)
} else {
    None
};

// TODO ELF flags from target_abi
