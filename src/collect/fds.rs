//! Recorrido único de los descriptores abiertos de un proceso.
//!
//! Tanto la medición de GPU como la de red necesitan saber a qué apunta
//! cada descriptor. Un navegador tiene miles: se leen una sola vez por
//! proceso y por muestra, y cada medición toma lo que le interesa.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

/// Lo que interesa de la tabla de descriptores de un proceso.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FdScan {
    /// Inodos de los sockets abiertos.
    pub sockets: HashSet<u64>,
    /// Nombres (números) de los descriptores sobre `/dev/dri/*`.
    pub drm: Vec<OsString>,
}

/// `socket:[12345]` → `12345`.
pub fn parse_socket_link(target: &str) -> Option<u64> {
    target
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Recorre `/proc/<pid>/fd`. `None` si no es legible (proceso ajeno o que
/// ya terminó).
pub fn scan(root: &Path, pid: i32) -> Option<FdScan> {
    let entries = fs::read_dir(root.join(pid.to_string()).join("fd")).ok()?;
    let mut out = FdScan::default();
    for entry in entries.flatten() {
        let Ok(target) = fs::read_link(entry.path()) else {
            continue;
        };
        if target.starts_with("/dev/dri") {
            out.drm.push(entry.file_name());
        } else if let Some(inode) = target.to_str().and_then(parse_socket_link) {
            out.sockets.insert(inode);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn parses_socket_links() {
        assert_eq!(parse_socket_link("socket:[48213]"), Some(48213));
        assert_eq!(parse_socket_link("pipe:[5]"), None);
        assert_eq!(parse_socket_link("/dev/null"), None);
        assert_eq!(parse_socket_link("socket:[x]"), None);
    }

    #[test]
    fn one_pass_finds_sockets_and_drm_descriptors() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("7").join("fd");
        fs::create_dir_all(&dir).unwrap();
        for (fd, target) in [
            (0, "/dev/null"),
            (3, "socket:[100]"),
            (4, "socket:[200]"),
            (5, "socket:[100]"),
            (8, "/dev/dri/renderD128"),
            (9, "/dev/dri/card1"),
            (10, "pipe:[77]"),
        ] {
            symlink(target, dir.join(fd.to_string())).unwrap();
        }
        let found = scan(tmp.path(), 7).unwrap();
        assert_eq!(found.sockets, HashSet::from([100, 200]));
        let mut drm = found.drm.clone();
        drm.sort();
        assert_eq!(drm, vec![OsString::from("8"), OsString::from("9")]);
        assert_eq!(scan(tmp.path(), 8), None);
    }

    #[test]
    fn scans_our_own_process() {
        let found = scan(Path::new("/proc"), std::process::id() as i32).unwrap();
        assert!(found.drm.is_empty());
    }
}
