use std::collections::BTreeSet;
use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use elb::host;
use elb::DynamicTag;
use elb::Elf;
use elb::ElfPatcher;
use elb_dl::DependencyTree;
use elb_dl::DynamicLoader;
use elb_dl::Libc;
use fs_err::File;
use fs_err::OpenOptions;

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
    } else {
        patcher.remove_interpreter()?;
    }
    patcher.set_library_search_path(DynamicTag::Runpath, c_rpath.as_c_str())?;
    patcher.finish()?;
    Ok(())
}

pub fn change_root<P1, P2>(file: P1, root: P2) -> Result<bool, elb_dl::Error>
where
    P1: AsRef<Path>,
    P2: Into<PathBuf>,
{
    let Some(host_class) = host::CLASS else {
        return Ok(false);
    };
    let Some(host_byte_order) = host::BYTE_ORDER else {
        return Ok(false);
    };
    let Some(host_machine) = host::MACHINE else {
        return Ok(false);
    };
    let root = root.into();
    let file = file.as_ref();
    let mut f = File::open(file)?;
    let elf = match Elf::read_unchecked(&mut f, 4096) {
        Ok(header) => header,
        Err(elb::Error::NotElf) => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    if !(elf.header.byte_order == host_byte_order
        && elf.header.class == host_class
        && elf.header.machine == host_machine)
    {
        return Ok(false);
    }
    let interpreter = elf.read_interpreter(&mut f)?;
    let Some(dynamic_table) = elf.read_dynamic_table(&mut f)? else {
        return Ok(false);
    };
    if dynamic_table.get(DynamicTag::Needed).is_none() {
        // Statically linked executable.
        return Ok(false);
    }
    drop(f);
    let mut tree = DependencyTree::new();
    let search_dirs = elb_dl::glibc::get_search_dirs(&root)?;
    let loader = DynamicLoader::options()
        .libc(Libc::Glibc)
        .search_dirs(search_dirs)
        .new_loader();
    let dependencies = loader.resolve_dependencies(file, &mut tree)?;
    // Change interpreter.
    let interpreter = if let Some(c_interpreter) = interpreter.as_ref() {
        let interpreter = Path::new(OsStr::from_bytes(c_interpreter.as_bytes()));
        let Ok(interpreter) = interpreter.strip_prefix("/") else {
            // Bad interpreter.
            return Ok(false);
        };
        let new_interpreter = canonicalize_in_root(&root.join(interpreter), &root)?;
        eprintln!("Set interp {:?}", new_interpreter);
        Some(new_interpreter)
    } else {
        None
    };
    // TODO modify existing rpath
    // Change rpath.
    let mut rpath = BTreeSet::new();
    for path in dependencies.iter() {
        rpath.insert(path.parent().expect("Parent exists").to_path_buf());
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
    Ok(true)
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
