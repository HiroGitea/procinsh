//! Local fixture acquisition for unprivileged-analysis tests.
pub(super) mod process {
    use crate::{
        http_server::analysis::snapshot::symbol::{ElfCache, ElfFile, ElfSymbols},
        http_server::model::memory::MemoryMap,
        ipc::schema::ProcessId,
    };
    use std::{
        fs,
        io::{BufRead, BufReader},
        os::unix::fs::MetadataExt,
        process::{Child, Command, Stdio},
        sync::Arc,
    };
    pub(crate) mod procfs {
        pub fn page_size() -> u64 {
            (unsafe { libc::sysconf(libc::_SC_PAGESIZE) }) as u64
        }
    }
    pub(crate) mod maps {
        pub use crate::http_server::model::memory::parse_map;
        pub fn read_maps(
            pid: i32,
        ) -> anyhow::Result<Vec<crate::http_server::model::memory::MemoryMap>> {
            fs::read_to_string(format!("/proc/{pid}/maps"))?
                .lines()
                .map(parse_map)
                .collect()
        }
        use std::fs;
    }
    pub fn load(cache: &mut ElfCache, pid: i32, map: &MemoryMap) -> Option<Arc<ElfSymbols>> {
        let path = format!("/proc/{pid}/root{}", map.pathname.as_deref()?);
        let meta = fs::metadata(&path).ok()?;
        cache.insert(
            map,
            ElfFile {
                identity: (
                    meta.dev(),
                    meta.ino(),
                    meta.len(),
                    meta.mtime(),
                    meta.mtime_nsec(),
                ),
                bytes: fs::read(path).ok()?,
            },
        )
    }
    pub struct TestTarget {
        pub id: ProcessId,
        child: Child,
    }
    impl TestTarget {
        pub fn new(name: &str) -> Self {
            static BUILD: std::sync::Once = std::sync::Once::new();
            BUILD.call_once(|| {
                assert!(
                    Command::new("sh")
                        .arg("tests/targets/build.sh")
                        .status()
                        .unwrap()
                        .success()
                )
            });
            let mut child = Command::new(format!("tests/targets/bin/{name}"))
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut line = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut line)
                .unwrap();
            let pid = child.id() as i32;
            let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
            let start = stat[stat.rfind(')').unwrap() + 1..]
                .split_whitespace()
                .nth(19)
                .unwrap()
                .parse()
                .unwrap();
            Self {
                id: ProcessId {
                    pid,
                    start_time_ticks: start,
                },
                child,
            }
        }
        pub fn assert_detached(&self) {
            for tid in fs::read_dir(format!("/proc/{}/task", self.id.pid))
                .unwrap()
                .flatten()
            {
                let status = fs::read_to_string(tid.path().join("status")).unwrap();
                assert!(status.lines().any(|line| line.trim() == "TracerPid:\t0"));
            }
        }
    }
    impl Drop for TestTarget {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
