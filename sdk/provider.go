package cybergrid

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"golang.org/x/sys/unix"
)

const androidProviderSHA = "c1f6fc493bef37eb530022a1598e689c0d50073680192616aa7aea4e732cdd3c"
const androidProviderSize int64 = 28812032
const macProviderSHA = "2d67b7315f96799123e19442580ce7c7616d6d7f322686fa829dd4e3fddfe715"
const macProviderSize int64 = 38893120

func privateStat(fd int, mode uint32, directory bool, own bool) error {
	var s unix.Stat_t
	if err := unix.Fstat(fd, &s); err != nil {
		return err
	}
	kind := uint32(unix.S_IFREG)
	if directory {
		kind = unix.S_IFDIR
	}
	if uint32(s.Mode)&unix.S_IFMT != kind || uint32(s.Mode)&07777 != mode || (own && int(s.Uid) != os.Geteuid()) {
		return errors.New("provider resource has unsafe type, owner or permissions")
	}
	return nil
}
func secureChild(parent int, name string) (int, error) {
	if err := unix.Mkdirat(parent, name, 0700); err != nil && err != unix.EEXIST {
		return -1, err
	}
	fd, err := unix.Openat(parent, name, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC|unix.O_NONBLOCK, 0)
	if err != nil {
		return -1, err
	}
	if err = privateStat(fd, 0700, true, true); err != nil {
		unix.Close(fd)
		return -1, err
	}
	return fd, nil
}
func openProviderLock(parent int) (int, error) {
	flags := unix.O_RDWR | unix.O_NOFOLLOW | unix.O_CLOEXEC | unix.O_NONBLOCK
	// Keep the lock inode forever. Separate opening from exclusive creation so
	// concurrent first users cannot replace or truncate each other's lease.
	for range 16 {
		fd, err := unix.Openat(parent, ".lock", flags, 0)
		if err == nil {
			return fd, nil
		}
		if err != unix.ENOENT {
			return -1, err
		}
		fd, err = unix.Openat(parent, ".lock", flags|unix.O_CREAT|unix.O_EXCL, 0600)
		if err == nil {
			return fd, nil
		}
		if err != unix.EEXIST {
			return -1, err
		}
		// Only a competing successful create permits reopening the fixed name.
	}
	return -1, errors.New("provider cache lock creation contention")
}
func verifyProviderFD(fd int, size int64, digest string, private bool) error {
	var s unix.Stat_t
	if err := unix.Fstat(fd, &s); err != nil {
		return err
	}
	if uint32(s.Mode)&unix.S_IFMT != unix.S_IFREG || s.Size != size {
		return errors.New("provider resource has wrong type or length")
	}
	if private {
		if err := privateStat(fd, 0700, false, true); err != nil {
			return err
		}
	} else if uint32(s.Mode)&07022 != 0 || uint32(s.Mode)&0111 == 0 || int(s.Uid) == os.Geteuid() {
		return errors.New("provider is not an executable installed Android resource")
	}
	// The caller owns fd; duplicate it so os.File cannot close the caller's handle.
	copyFD, err := unix.Dup(fd)
	if err != nil {
		return err
	}
	f := os.NewFile(uintptr(copyFD), "provider-verification")
	defer f.Close()
	hash := sha256.New()
	if _, err = io.Copy(hash, f); err != nil {
		return err
	}
	if hex.EncodeToString(hash.Sum(nil)) != digest {
		return errors.New("provider resource checksum mismatch")
	}
	return nil
}
func extractProvider(ctx context.Context, base string, data []byte, platform, digest string, size int64) (string, error) {
	if len(data) != int(size) || fmt.Sprintf("%x", sha256.Sum256(data)) != digest {
		return "", errors.New("embedded provider checksum mismatch")
	}
	if base == "" {
		var err error
		base, err = os.UserCacheDir()
		if err != nil {
			return "", err
		}
	}
	if !filepath.IsAbs(base) {
		return "", errors.New("provider cache base must be absolute")
	}
	if err := os.MkdirAll(base, 0700); err != nil {
		return "", err
	}
	real, err := filepath.EvalSymlinks(base)
	if err != nil {
		return "", err
	}
	fd, err := unix.Open(real, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC|unix.O_NONBLOCK, 0)
	if err != nil {
		return "", err
	}
	defer func() { unix.Close(fd) }()
	var bs unix.Stat_t
	if err = unix.Fstat(fd, &bs); err != nil {
		return "", err
	}
	if int(bs.Uid) != os.Geteuid() || uint32(bs.Mode)&07022 != 0 {
		return "", errors.New("unsafe provider cache base")
	}
	names := []string{"cybergrid-providers", "2026.9.0", platform, digest}
	for _, name := range names {
		next, e := secureChild(fd, name)
		if e != nil {
			return "", e
		}
		unix.Close(fd)
		fd = next
		real = filepath.Join(real, name)
	}
	lock, err := openProviderLock(fd)
	if err != nil {
		return "", err
	}
	defer unix.Close(lock)
	if err = privateStat(lock, 0600, false, true); err != nil {
		return "", err
	}
	deadline := time.Now().Add(30 * time.Second)
	for {
		err = unix.Flock(lock, unix.LOCK_EX|unix.LOCK_NB)
		if err == nil {
			break
		}
		if err != unix.EWOULDBLOCK && err != unix.EAGAIN {
			return "", err
		}
		if time.Now().After(deadline) {
			return "", errors.New("provider cache lock busy")
		}
		select {
		case <-ctx.Done():
			return "", ctx.Err()
		case <-time.After(20 * time.Millisecond):
		}
	}
	defer unix.Flock(lock, unix.LOCK_UN)
	existing, e := unix.Openat(fd, "cloudflared", unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_CLOEXEC|unix.O_NONBLOCK, 0)
	if e == nil {
		defer unix.Close(existing)
		if e = verifyProviderFD(existing, size, digest, true); e != nil {
			return "", e
		}
		return filepath.Join(real, "cloudflared"), nil
	}
	if e != unix.ENOENT {
		return "", e
	}
	random := make([]byte, 16)
	if _, err = rand.Read(random); err != nil {
		return "", err
	}
	temp := ".cloudflared-" + hex.EncodeToString(random)
	out, err := unix.Openat(fd, temp, unix.O_WRONLY|unix.O_CREAT|unix.O_EXCL|unix.O_NOFOLLOW|unix.O_CLOEXEC|unix.O_NONBLOCK, 0600)
	if err != nil {
		return "", err
	}
	defer unix.Unlinkat(fd, temp, 0)
	file := os.NewFile(uintptr(out), "provider-temporary")
	defer file.Close()
	hash := sha256.New()
	if _, err = io.Copy(io.MultiWriter(file, hash), bytes.NewReader(data)); err != nil {
		return "", err
	}
	if hex.EncodeToString(hash.Sum(nil)) != digest {
		return "", errors.New("provider extraction checksum mismatch")
	}
	if err = file.Chmod(0700); err != nil {
		return "", err
	}
	if err = file.Sync(); err != nil {
		return "", err
	}
	if err = file.Close(); err != nil {
		return "", err
	}
	if err = unix.Renameat(fd, temp, fd, "cloudflared"); err != nil {
		return "", err
	}
	if err = unix.Fsync(fd); err != nil {
		return "", err
	}
	return filepath.Join(real, "cloudflared"), nil
}
func installedAndroidPath(dir string) (string, error) {
	if !filepath.IsAbs(dir) || strings.Contains(dir, "\\") {
		return "", errors.New("absolute Android nativeLibraryDir required")
	}
	for _, part := range strings.Split(dir, "/") {
		if part == "." || part == ".." {
			return "", errors.New("invalid Android nativeLibraryDir")
		}
	}
	clean := filepath.Clean(dir)
	installed := strings.HasPrefix(clean, "/data/app/") || strings.HasPrefix(clean, "/system/") || strings.HasPrefix(clean, "/product/") || strings.HasPrefix(clean, "/vendor/")
	if strings.HasPrefix(clean, "/mnt/expand/") {
		parts := strings.Split(strings.TrimPrefix(clean, "/mnt/expand/"), "/")
		installed = len(parts) >= 3 && parts[0] != "" && parts[1] == "app"
	}
	if !installed {
		return "", errors.New("provider must be in Android installed nativeLibraryDir")
	}
	fd, err := unix.Open(clean, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC|unix.O_NONBLOCK, 0)
	if err != nil {
		return "", err
	}
	defer unix.Close(fd)
	var s unix.Stat_t
	if err = unix.Fstat(fd, &s); err != nil {
		return "", err
	}
	if int(s.Uid) == os.Geteuid() || uint32(s.Mode)&07022 != 0 {
		return "", errors.New("Android nativeLibraryDir is writable or app-owned")
	}
	binary, err := unix.Openat(fd, "libcloudflared.so", unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_CLOEXEC|unix.O_NONBLOCK, 0)
	if err != nil {
		return "", err
	}
	defer unix.Close(binary)
	if err = verifyProviderFD(binary, androidProviderSize, androidProviderSHA, false); err != nil {
		return "", err
	}
	return filepath.Join(clean, "libcloudflared.so"), nil
}
func (p Cloudflared) executable(ctx context.Context) (string, error) {
	if p.Binary != "" {
		return exec.LookPath(p.Binary)
	} // Explicit trusted external override.
	if p.NativeLibraryDir != "" {
		if runtime.GOOS != "android" || runtime.GOARCH != "arm64" {
			return "", errors.New("bundled Android provider unsupported on this platform")
		}
		return installedAndroidPath(p.NativeLibraryDir)
	}
	if runtime.GOOS != "darwin" || runtime.GOARCH != "arm64" {
		return "", errors.New("bundled provider unavailable: Android requires installed NativeLibraryDir; supported platforms are darwin/arm64 and android/arm64")
	}
	return extractProvider(ctx, p.CacheDir, macProviderBytes(), "darwin-arm64", macProviderSHA, macProviderSize)
}
