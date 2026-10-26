//! Clear HTTP process capabilities and prohibit filesystem access after helper startup.
//!
//! # Interface
//! All entry points have `pub(super)` visibility.
//! - [`close_inherited`]: `fn close_inherited() -> anyhow::Result<()>`.
//! - [`drop_application_capabilities`]: `fn drop_application_capabilities() -> anyhow::Result<()>`.
//! - [`seal_application`]: `fn seal_application() -> anyhow::Result<()>`.
use anyhow::{Context, Result, ensure};
use std::io;

// Linux capability ABI v3 covers capabilities 0..63 on x86-64.
#[repr(C)]
struct CapabilityHeader {
    version: u32,
    pid: i32,
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct CapabilitySet {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}

fn check(result: libc::c_long, operation: &str) -> Result<()> {
    if result < 0 {
        return Err(io::Error::last_os_error()).context(operation.to_owned());
    }
    Ok(())
}
fn capabilities() -> Result<[CapabilitySet; 2]> {
    let mut header = CapabilityHeader {
        version: 0x20080522,
        pid: 0,
    };
    let mut data = [CapabilitySet::default(); 2];
    check(
        unsafe { libc::syscall(libc::SYS_capget, &mut header, data.as_mut_ptr()) },
        "capget",
    )?;
    Ok(data)
}
fn set_capabilities(data: &[CapabilitySet; 2]) -> Result<()> {
    let header = CapabilityHeader {
        version: 0x20080522,
        pid: 0,
    };
    check(
        unsafe { libc::syscall(libc::SYS_capset, &header, data.as_ptr()) },
        "capset",
    )
}
fn prctl(option: i32, arg: libc::c_ulong, operation: &str) -> Result<()> {
    check(
        unsafe { libc::prctl(option, arg, 0, 0, 0) } as libc::c_long,
        operation,
    )
}

/// Close inherited resources before creating the socketpair or any runtime.
/// Preserve only stdio; requires Linux close_range support.
pub(super) fn close_inherited() -> Result<()> {
    check(
        unsafe { libc::syscall(libc::SYS_close_range, 3_u32, u32::MAX, 0_u32) },
        "close inherited resources",
    )
}

fn clear_ambient() -> Result<()> {
    prctl(
        libc::PR_CAP_AMBIENT,
        libc::PR_CAP_AMBIENT_CLEAR_ALL as _,
        "clear ambient capabilities",
    )
}

pub(super) fn drop_application_capabilities() -> Result<()> {
    clear_ambient()?;
    set_capabilities(&[CapabilitySet::default(); 2])?;
    // Do not let ordinary same-UID processes inspect or tamper with the IPC
    // client. This also closes the /proc/PID/fd access route to the socketpair.
    prctl(libc::PR_SET_DUMPABLE, 0, "disable HTTP worker dumps")?;
    let data = capabilities()?;
    ensure!(
        data.iter()
            .all(|s| s.permitted == 0 && s.effective == 0 && s.inheritable == 0),
        "HTTP worker retained capabilities"
    );
    Ok(())
}

pub(super) fn seal_application() -> Result<()> {
    prctl(libc::PR_SET_NO_NEW_PRIVS, 1, "set application no_new_privs")?;
    deny_path_access()
}

/// After helper startup, allow inherited log descriptors and IPC/network sockets,
/// but reject filesystem opens and pathname inspection. New threads inherit this filter.
fn deny_path_access() -> Result<()> {
    #[cfg(target_arch = "x86_64")]
    let architecture = 0xc000003e;
    #[cfg(target_arch = "aarch64")]
    let architecture = 0xc00000b7;
    let denied = vec![
        libc::SYS_openat,
        libc::SYS_openat2,
        libc::SYS_readlinkat,
        libc::SYS_newfstatat,
        libc::SYS_statx,
        libc::SYS_faccessat,
        libc::SYS_faccessat2,
        libc::SYS_getdents64,
        libc::SYS_unlinkat,
        libc::SYS_renameat,
        libc::SYS_renameat2,
        libc::SYS_mkdirat,
        libc::SYS_mknodat,
        libc::SYS_linkat,
        libc::SYS_symlinkat,
        libc::SYS_fchmodat,
        libc::SYS_fchownat,
        libc::SYS_utimensat,
        libc::SYS_execve,
        libc::SYS_execveat,
        libc::SYS_io_uring_setup,
        libc::SYS_io_uring_enter,
        libc::SYS_io_uring_register,
        libc::SYS_open_by_handle_at,
        libc::SYS_name_to_handle_at,
        libc::SYS_inotify_add_watch,
        libc::SYS_chdir,
        libc::SYS_chroot,
        libc::SYS_statfs,
        libc::SYS_getxattr,
        libc::SYS_lgetxattr,
        libc::SYS_listxattr,
        libc::SYS_llistxattr,
        libc::SYS_setxattr,
        libc::SYS_lsetxattr,
        libc::SYS_removexattr,
        libc::SYS_lremovexattr,
    ];
    #[cfg(target_arch = "x86_64")]
    let denied = {
        let mut denied = denied;
        denied.extend([
            libc::SYS_open,
            libc::SYS_creat,
            libc::SYS_stat,
            libc::SYS_lstat,
            libc::SYS_access,
            libc::SYS_readlink,
            libc::SYS_getdents,
            libc::SYS_unlink,
            libc::SYS_rename,
            libc::SYS_mkdir,
            libc::SYS_rmdir,
            libc::SYS_link,
            libc::SYS_symlink,
            libc::SYS_chmod,
            libc::SYS_chown,
            libc::SYS_lchown,
            libc::SYS_mknod,
            libc::SYS_utime,
            libc::SYS_utimes,
        ]);
        denied
    };
    let statement = |code, k| libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    };
    let jump = |k, jt, jf| libc::sock_filter {
        code: 0x15,
        jt,
        jf,
        k,
    };
    // seccomp_data: architecture at byte 4, syscall number at byte 0.
    let mut filter = vec![
        statement(0x20, 4),
        jump(architecture, 1, 0),
        statement(0x06, 0x80000000),
        statement(0x20, 0),
    ];
    #[cfg(target_arch = "x86_64")]
    {
        // x32 shares the audit architecture but uses a separate syscall number space.
        filter.push(libc::sock_filter {
            code: 0x45,
            jt: 0,
            jf: 1,
            k: 0x40000000,
        });
        filter.push(statement(0x06, 0x80000000));
    }
    for number in denied {
        filter.push(jump(number as u32, 0, 1));
        filter.push(statement(0x06, 0x00050000 | libc::EPERM as u32));
    }
    filter.push(statement(0x06, 0x7fff0000));
    let program = libc::sock_fprog {
        len: filter.len().try_into()?,
        filter: filter.as_mut_ptr(),
    };
    check(
        unsafe { libc::syscall(libc::SYS_seccomp, 1_u32, 0_u32, &program) },
        "deny HTTP filesystem access",
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn http_file_access_is_denied_without_affecting_sockets_or_logs() {
        const CHILD: &str = "PROCINSH_TEST_FILE_POLICY";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "http_server::privileges::tests::http_file_access_is_denied_without_affecting_sockets_or_logs", "--nocapture"])
                .env(CHILD, "1").output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("inherited stderr remains available for logs")
            );
            return;
        }
        super::seal_application().unwrap();
        assert_eq!(
            std::fs::read("/etc/passwd").unwrap_err().raw_os_error(),
            Some(libc::EPERM)
        );
        assert_eq!(
            std::fs::metadata("/proc/self").unwrap_err().raw_os_error(),
            Some(libc::EPERM)
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let connection = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let _ = listener.accept().unwrap();
        drop(connection);
        eprintln!("inherited stderr remains available for logs");
    }
}
