//! Fixed bundled provider resources. No runtime downloads or PATH discovery.
use crate::{Error, Result};
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) const MAC_SHA: &str = "2d67b7315f96799123e19442580ce7c7616d6d7f322686fa829dd4e3fddfe715";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) const MAC_SIZE: u64 = 38_893_120;
pub(crate) const ANDROID_SHA: &str =
    "c1f6fc493bef37eb530022a1598e689c0d50073680192616aa7aea4e732cdd3c";
pub(crate) const ANDROID_SIZE: u64 = 28_812_032;
#[derive(Clone)]
pub(crate) enum ProviderSource {
    External,
    Mac(PathBuf),
    Android(PathBuf),
}
fn bad(s: &str) -> Error {
    Error::Config(s.into())
}
fn open_at(parent: &File, name: &str, flags: i32, mode: u32) -> Result<File> {
    let c = CString::new(name).map_err(|_| bad("invalid provider filename"))?;
    // SAFETY: parent is held open, CString remains valid, the returned fd is owned once.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            c.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn private(file: &File, mode: u32, dir: bool) -> Result<()> {
    let m = file.metadata()?;
    if (dir && !m.is_dir())
        || (!dir && !m.is_file())
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o7777 != mode
    {
        return Err(bad("unsafe provider resource type, owner or permissions"));
    }
    Ok(())
}
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
fn child(parent: &File, name: &str) -> Result<File> {
    let c = CString::new(name).map_err(|_| bad("invalid provider directory"))?;
    let status = unsafe { libc::mkdirat(parent.as_raw_fd(), c.as_ptr(), 0o700) };
    if status < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
        return Err(std::io::Error::last_os_error().into());
    }
    let next = open_at(parent, name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
    private(&next, 0o700, true)?;
    Ok(next)
}
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
fn open_lock(parent: &File) -> Result<File> {
    // Never unlink the lease. Only a competing exclusive create permits a
    // bounded reopen of this fixed name; other filesystem errors fail closed.
    for _ in 0..16 {
        match open_at(parent, ".lock", libc::O_RDWR, 0) {
            Ok(file) => return Ok(file),
            Err(Error::Io(e)) if e.raw_os_error() == Some(libc::ENOENT) => {}
            Err(e) => return Err(e),
        }
        match open_at(
            parent,
            ".lock",
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600,
        ) {
            Ok(file) => return Ok(file),
            Err(Error::Io(e)) if e.raw_os_error() == Some(libc::EEXIST) => {}
            Err(e) => return Err(e),
        }
    }
    Err(bad("provider cache lock creation contention"))
}
fn verify(file: &mut File, size: u64, sha: &str, own: bool) -> Result<()> {
    let m = file.metadata()?;
    if !m.is_file() || m.len() != size {
        return Err(bad("provider resource has wrong type or length"));
    }
    if own {
        private(file, 0o700, false)?;
    } else if m.mode() & 0o7022 != 0
        || m.mode() & 0o111 == 0
        || m.uid() == unsafe { libc::geteuid() }
    {
        return Err(bad(
            "provider is not an executable installed Android resource",
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if hex::encode(hash.finalize()) != sha {
        return Err(bad("provider resource checksum mismatch"));
    }
    Ok(())
}
#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
pub(crate) fn extract(
    base: &Path,
    data: &[u8],
    target: &str,
    sha: &str,
    size: u64,
) -> Result<PathBuf> {
    if data.len() as u64 != size || hex::encode(Sha256::digest(data)) != sha {
        return Err(bad("embedded provider checksum mismatch"));
    }
    if !base.is_absolute() {
        return Err(bad("provider cache base must be absolute"));
    }
    std::fs::create_dir_all(base)?;
    let mut path = std::fs::canonicalize(base)?;
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)?;
    let m = directory.metadata()?;
    if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7022 != 0 {
        return Err(bad("unsafe provider cache base"));
    }
    for name in ["cybergrid-providers", "2026.9.0", target, sha] {
        directory = child(&directory, name)?;
        path.push(name);
    }
    let lock = open_lock(&directory)?;
    private(&lock, 0o600, false)?;
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        match lock.try_lock_exclusive() {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > until {
                    return Err(bad("provider cache lock busy"));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.into()),
        }
    }
    let result = (|| {
        match open_at(&directory, "cloudflared", libc::O_RDONLY, 0) {
            Ok(mut file) => {
                verify(&mut file, size, sha, true)?;
                return Ok(path.join("cloudflared"));
            }
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let temporary = format!(".cloudflared-{}", uuid::Uuid::new_v4().simple());
        let output = open_at(
            &directory,
            &temporary,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        let result = (|| {
            let mut output = output;
            let mut hash = Sha256::new();
            for part in data.chunks(65536) {
                output.write_all(part)?;
                hash.update(part);
            }
            if hex::encode(hash.finalize()) != sha {
                return Err(bad("provider extraction checksum mismatch"));
            }
            output.set_permissions(std::fs::Permissions::from_mode(0o700))?;
            output.sync_all()?;
            drop(output);
            let from =
                CString::new(temporary.clone()).map_err(|_| bad("invalid temporary path"))?;
            let to = CString::new("cloudflared").expect("constant");
            let status = unsafe {
                libc::renameat(
                    directory.as_raw_fd(),
                    from.as_ptr(),
                    directory.as_raw_fd(),
                    to.as_ptr(),
                )
            };
            if status < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            directory.sync_all()?;
            Ok(path.join("cloudflared"))
        })();
        let name = CString::new(temporary).expect("UUID filename");
        unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
        result
    })();
    FileExt::unlock(&lock)?;
    result
}
pub(crate) fn android_path(dir: &Path) -> Result<PathBuf> {
    let text = dir
        .to_str()
        .ok_or_else(|| bad("invalid nativeLibraryDir"))?;
    if !dir.is_absolute() || text.contains('\\') || text.split('/').any(|p| p == "." || p == "..") {
        return Err(bad("absolute installed nativeLibraryDir required"));
    }
    let installed = ["/data/app/", "/system/", "/product/", "/vendor/"]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        || text.strip_prefix("/mnt/expand/").is_some_and(|s| {
            let p = s.split('/').collect::<Vec<_>>();
            p.len() >= 3 && !p[0].is_empty() && p[1] == "app"
        });
    if !installed {
        return Err(bad(
            "provider must be in Android installed nativeLibraryDir",
        ));
    }
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(dir)?;
    let m = directory.metadata()?;
    if m.uid() == unsafe { libc::geteuid() } || m.mode() & 0o7022 != 0 {
        return Err(bad("Android nativeLibraryDir is writable or app-owned"));
    }
    let mut binary = open_at(&directory, "libcloudflared.so", libc::O_RDONLY, 0)?;
    verify(&mut binary, ANDROID_SIZE, ANDROID_SHA, false)?;
    Ok(dir.join("libcloudflared.so"))
}
pub(crate) fn mac_path(cache: &Path) -> Result<PathBuf> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        extract(
            cache,
            include_bytes!("../provider-assets/darwin-arm64/cloudflared"),
            "darwin-arm64",
            MAC_SHA,
            MAC_SIZE,
        )
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = cache;
        Err(bad("bundled macOS provider supports only darwin/arm64"))
    }
}
pub(crate) fn resolve(source: &ProviderSource, external: &Path) -> Result<PathBuf> {
    match source {
        ProviderSource::External => Ok(external.to_path_buf()),
        ProviderSource::Mac(cache) => mac_path(cache),
        ProviderSource::Android(dir) => {
            if !cfg!(all(target_os = "android", target_arch = "aarch64")) {
                return Err(bad("bundled Android provider supports only android/arm64"));
            }
            android_path(dir)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    fn fixture() -> (tempfile::TempDir, Vec<u8>, String) {
        let base = tempfile::tempdir().unwrap();
        std::fs::set_permissions(base.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let data = b"pinned executable resource fixture\n".to_vec();
        let sha = hex::encode(Sha256::digest(&data));
        (base, data, sha)
    }
    #[test]
    fn cache_first_reuse_and_parallel_processes() {
        if let Some(base) = std::env::var_os("CG_RUST_PROVIDER_TEST_BASE") {
            let data = b"pinned executable resource fixture\n";
            extract(
                Path::new(&base),
                data,
                "fixture",
                &hex::encode(Sha256::digest(data)),
                data.len() as u64,
            )
            .unwrap();
            return;
        }
        let (base, data, sha) = fixture();
        let mut children = Vec::new();
        for _ in 0..5 {
            children.push(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("provider::tests::cache_first_reuse_and_parallel_processes")
                    .arg("--exact")
                    .env("CG_RUST_PROVIDER_TEST_BASE", base.path())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
        }
        let outputs: Vec<_> = children
            .into_iter()
            .map(|child| child.wait_with_output())
            .collect();
        for output in outputs {
            let output = output.unwrap();
            assert!(
                output.status.success(),
                "child {:?}: {} {}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let p = extract(base.path(), &data, "fixture", &sha, data.len() as u64).unwrap();
        let first = std::fs::metadata(&p).unwrap();
        let lock = p.parent().unwrap().join(".lock");
        let first_lock = std::fs::metadata(&lock).unwrap();
        assert_eq!(
            extract(base.path(), &data, "fixture", &sha, data.len() as u64).unwrap(),
            p
        );
        assert_eq!(first.ino(), std::fs::metadata(&p).unwrap().ino());
        assert_eq!(first_lock.ino(), std::fs::metadata(&lock).unwrap().ino());
        assert_eq!(first.mode() & 0o777, 0o700);
        assert_eq!(first_lock.mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_dir(p.parent().unwrap()).unwrap().count(), 2);
    }
    #[test]
    fn cache_rejects_damage_symlinks_and_permissions() {
        for kind in [
            "fifo",
            "lock-fifo",
            "binary-special",
            "lock-special",
            "directory-special",
            "base-special",
            "hash",
            "length",
            "binary-link",
            "lock-link",
            "root-link",
            "directory-mode",
            "binary-mode",
            "lock-mode",
        ] {
            let (base, data, sha) = fixture();
            let p = extract(base.path(), &data, "fixture", &sha, data.len() as u64).unwrap();
            let outside = tempfile::tempdir().unwrap();
            let file = outside.path().join("outside");
            std::fs::write(&file, &data).unwrap();
            let lock = p.parent().unwrap().join(".lock");
            match kind {
                "fifo" | "lock-fifo" => {
                    let target = if kind == "fifo" { &p } else { &lock };
                    std::fs::remove_file(target).unwrap();
                    let path =
                        CString::new(std::os::unix::ffi::OsStrExt::as_bytes(target.as_os_str()))
                            .unwrap();
                    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o700) }, 0);
                }
                "binary-special" => {
                    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o4700)).unwrap()
                }
                "lock-special" => {
                    std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o2600))
                        .unwrap()
                }
                "directory-special" => std::fs::set_permissions(
                    p.parent().unwrap(),
                    std::fs::Permissions::from_mode(0o1700),
                )
                .unwrap(),
                "base-special" => {
                    std::fs::set_permissions(base.path(), std::fs::Permissions::from_mode(0o1755))
                        .unwrap()
                }
                "hash" => {
                    let mut wrong = data.clone();
                    wrong[0] ^= 1;
                    std::fs::write(&p, wrong).unwrap();
                }
                "length" => std::fs::write(&p, b"bad").unwrap(),
                "binary-link" => {
                    std::fs::remove_file(&p).unwrap();
                    symlink(&file, &p).unwrap();
                }
                "lock-link" => {
                    std::fs::remove_file(&lock).unwrap();
                    symlink(&file, &lock).unwrap();
                }
                "root-link" => {
                    let root = base.path().join("cybergrid-providers");
                    std::fs::remove_dir_all(&root).unwrap();
                    symlink(outside.path(), root).unwrap();
                }
                "directory-mode" => std::fs::set_permissions(
                    p.parent().unwrap(),
                    std::fs::Permissions::from_mode(0o755),
                )
                .unwrap(),
                "binary-mode" => {
                    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap()
                }
                "lock-mode" => {
                    std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o644)).unwrap()
                }
                _ => unreachable!(),
            }
            assert!(
                extract(base.path(), &data, "fixture", &sha, data.len() as u64).is_err(),
                "accepted {kind}"
            );
            assert_eq!(std::fs::read(file).unwrap(), data);
        }
    }
    #[test]
    fn android_rejects_writable_and_traversal_paths() {
        for path in [
            "relative",
            "/data/local/tmp",
            "/data/user/0/app/files",
            "/data/app/test/../lib",
            "/data/app/test/./lib",
            "/mnt/expand/id/user/app",
        ] {
            assert!(android_path(Path::new(path)).is_err(), "accepted {path}");
        }
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn bundled_mac_offline_and_every_resolution() {
        let base = tempfile::tempdir().unwrap();
        let config = crate::CloudflaredConfig::bundled(base.path()).unwrap();
        let result = std::process::Command::new(&config.binary)
            .arg("--version")
            .env_clear()
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains("2026.9.0"));
        assert_eq!(
            resolve(&config.source, &config.binary).unwrap(),
            config.binary
        );
        std::fs::set_permissions(&config.binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(resolve(&config.source, &config.binary).is_err());
    }
}
