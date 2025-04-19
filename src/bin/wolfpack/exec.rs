use std::ffi::CString;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitCode;

use libc::c_long;
use libc::getgid;
use libc::getuid;
use libc::mount;
use libc::syscall;
use libc::unshare;
use libc::SYS_pivot_root;
use libc::CLONE_NEWNS;
use libc::CLONE_NEWUSER;
use libc::MS_BIND;
use libc::MS_PRIVATE;
use libc::MS_REC;

#[derive(clap::Args)]
pub struct ExecArgs {
    /// File system root containing the program to run.
    #[clap(short = 'r', long = "root", value_name = "DIR")]
    rootfs_dir: PathBuf,

    /// Clear environment variables before running the program.
    #[clap(action, short = 'E', long = "clear-env")]
    clear_env: bool,

    /// Set environment variables.
    #[clap(short = 'e', long = "env", value_name = "NAME[=VALUE]...")]
    env: Vec<OsString>,

    /// Program to run.
    program: PathBuf,

    /// Program arguments.
    #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<OsString>,
}

pub fn exec(args: ExecArgs) -> Result<ExitCode, std::io::Error> {
    if !args.rootfs_dir.is_absolute() {
        return Err(std::io::Error::other(format!(
            "{:?} must be an absolute path",
            args.rootfs_dir
        )));
    }
    let uid = unsafe { getuid() };
    let gid = unsafe { getgid() };
    check(unsafe { unshare(CLONE_NEWNS | CLONE_NEWUSER) })?;
    fs_err::write("/proc/self/uid_map", format!("0 {uid} 1"))?;
    fs_err::write("/proc/self/setgroups", "deny")?;
    fs_err::write("/proc/self/gid_map", format!("0 {gid} 1"))?;
    check(unsafe {
        mount(
            c"none".as_ptr(),
            c"/".as_ptr(),
            std::ptr::null(),
            MS_REC | MS_PRIVATE,
            std::ptr::null(),
        )
    })?;
    let old_root_dir = args.rootfs_dir.join(".wolfpack-exec-old");
    fs_err::create_dir_all(&old_root_dir)?;
    let root_dir_bind = args.rootfs_dir.join(
        args.rootfs_dir
            .strip_prefix("/")
            .expect("Checked that the path is absolute above"),
    );
    let c_old_root_dir = into_c_string(old_root_dir);
    let c_rootfs_dir = into_c_string(args.rootfs_dir);
    check(unsafe {
        mount(
            c_rootfs_dir.as_c_str().as_ptr(),
            c_rootfs_dir.as_c_str().as_ptr(),
            c"none".as_ptr(),
            MS_BIND,
            std::ptr::null(),
        )
    })?;
    check(unsafe {
        syscall(
            SYS_pivot_root,
            c_rootfs_dir.as_c_str().as_ptr(),
            c_old_root_dir.as_c_str().as_ptr(),
        )
    })?;
    fs_err::create_dir_all(&root_dir_bind)?;
    //let c_root_dir_bind = into_c_string(root_dir_bind);
    check(unsafe {
        mount(
            c"/".as_ptr(),
            c_rootfs_dir.as_c_str().as_ptr(),
            c"none".as_ptr(),
            MS_BIND,
            std::ptr::null(),
        )
    })?;
    //fs_err::create_dir_all("/proc")?;
    //check(unsafe {
    //    mount(
    //        c"proc".as_ptr(),
    //        c"/proc".as_ptr(),
    //        c"proc".as_ptr(),
    //        MS_NOSUID | MS_NODEV | MS_NOEXEC,
    //        std::ptr::null(),
    //    )
    //})?;
    let mut command = Command::new(&args.program);
    command.args(&args.args);
    if args.clear_env {
        command.env_clear();
    }
    for var in args.env.iter() {
        let bytes = var.as_encoded_bytes();
        match bytes.iter().position(|b| *b == b'=') {
            Some(i) => {
                let name = unsafe { OsStr::from_encoded_bytes_unchecked(&bytes[..i]) };
                let value = unsafe { OsStr::from_encoded_bytes_unchecked(&bytes[i + 1..]) };
                command.env(name, value);
            }
            None => {
                let name = var;
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
        }
    }
    Err(command.exec())
}

#[inline]
fn check<T: Into<c_long> + Copy>(ret: T) -> Result<T, std::io::Error> {
    if ret.into() == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

fn into_c_string(path: PathBuf) -> CString {
    let mut bytes = path.into_os_string().into_vec();
    bytes.push(0_u8);
    unsafe { CString::from_vec_with_nul_unchecked(bytes) }
}
