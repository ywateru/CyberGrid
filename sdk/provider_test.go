package cybergrid

import (
	"context"
	"crypto/sha256"
	"fmt"
	"golang.org/x/sys/unix"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
)

func fixtureProvider(t *testing.T) (string, []byte, string) {
	t.Helper()
	base := t.TempDir()
	if err := os.Chmod(base, 0755); err != nil {
		t.Fatal(err)
	}
	data := []byte("pinned executable resource fixture\n")
	return base, data, fmt.Sprintf("%x", sha256.Sum256(data))
}
func resolveFixture(t *testing.T, base string, data []byte, sha string) string {
	t.Helper()
	p, e := extractProvider(context.Background(), base, data, "fixture", sha, int64(len(data)))
	if e != nil {
		t.Fatal(e)
	}
	return p
}
func TestBundledCacheReuseAndConcurrentProcesses(t *testing.T) {
	if os.Getenv("CG_PROVIDER_TEST_CHILD") == "1" {
		data := []byte("pinned executable resource fixture\n")
		_, e := extractProvider(context.Background(), os.Getenv("CG_PROVIDER_TEST_BASE"), data, "fixture", fmt.Sprintf("%x", sha256.Sum256(data)), int64(len(data)))
		if e != nil {
			t.Fatal(e)
		}
		return
	}
	base, data, sha := fixtureProvider(t)
	const n = 5
	var wg sync.WaitGroup
	errs := make(chan error, n)
	for range n {
		wg.Add(1)
		go func() {
			defer wg.Done()
			cmd := exec.Command(os.Args[0], "-test.run=^TestBundledCacheReuseAndConcurrentProcesses$")
			cmd.Env = append(os.Environ(), "CG_PROVIDER_TEST_CHILD=1", "CG_PROVIDER_TEST_BASE="+base)
			if out, e := cmd.CombinedOutput(); e != nil {
				errs <- fmt.Errorf("child: %v %s", e, out)
			}
		}()
	}
	wg.Wait()
	close(errs)
	for e := range errs {
		t.Error(e)
	}
	p := resolveFixture(t, base, data, sha)
	a, _ := os.Stat(p)
	lock, _ := os.Stat(filepath.Join(filepath.Dir(p), ".lock"))
	p2 := resolveFixture(t, base, data, sha)
	b, _ := os.Stat(p2)
	lock2, _ := os.Stat(filepath.Join(filepath.Dir(p), ".lock"))
	if !os.SameFile(a, b) || !os.SameFile(lock, lock2) {
		t.Fatal("binary/lease inode changed on reuse")
	}
	if a.Mode().Perm() != 0700 || lock.Mode().Perm() != 0600 {
		t.Fatal("unsafe modes")
	}
	files, _ := os.ReadDir(filepath.Dir(p))
	if len(files) != 2 {
		t.Fatal("temporary extraction file leaked")
	}
}
func TestBundledCacheRejectsDamageSymlinksAndPermissions(t *testing.T) {
	for _, kind := range []string{"binary-fifo", "lock-fifo", "binary-special", "lock-special", "directory-special", "base-special", "wrong-hash", "wrong-length", "binary-symlink", "lock-symlink", "root-symlink", "directory-mode", "binary-mode", "lock-mode"} {
		t.Run(kind, func(t *testing.T) {
			base, data, sha := fixtureProvider(t)
			p := resolveFixture(t, base, data, sha)
			lock := filepath.Join(filepath.Dir(p), ".lock")
			outside := filepath.Join(t.TempDir(), "outside")
			if e := os.WriteFile(outside, data, 0700); e != nil {
				t.Fatal(e)
			}
			switch kind {
			case "binary-fifo":
				os.Remove(p)
				if e := unix.Mkfifo(p, 0700); e != nil {
					t.Fatal(e)
				}
			case "lock-fifo":
				os.Remove(lock)
				if e := unix.Mkfifo(lock, 0600); e != nil {
					t.Fatal(e)
				}
			case "binary-special":
				os.Chmod(p, 0700|os.ModeSetuid)
			case "lock-special":
				os.Chmod(lock, 0600|os.ModeSetgid)
			case "directory-special":
				os.Chmod(filepath.Dir(p), 0700|os.ModeSticky)
			case "base-special":
				os.Chmod(base, 0755|os.ModeSticky)
			case "wrong-hash":
				bad := append([]byte{}, data...)
				bad[0] ^= 1
				os.WriteFile(p, bad, 0700)
			case "wrong-length":
				os.WriteFile(p, []byte("bad"), 0700)
			case "binary-symlink":
				os.Remove(p)
				os.Symlink(outside, p)
			case "lock-symlink":
				os.Remove(lock)
				os.Symlink(outside, lock)
			case "root-symlink":
				os.RemoveAll(filepath.Join(base, "cybergrid-providers"))
				os.Symlink(filepath.Dir(outside), filepath.Join(base, "cybergrid-providers"))
			case "directory-mode":
				os.Chmod(filepath.Dir(p), 0755)
			case "binary-mode":
				os.Chmod(p, 0755)
			case "lock-mode":
				os.Chmod(lock, 0644)
			}
			if _, e := extractProvider(context.Background(), base, data, "fixture", sha, int64(len(data))); e == nil {
				t.Fatal("unsafe resource accepted")
			}
			got, _ := os.ReadFile(outside)
			if string(got) != string(data) {
				t.Fatal("symlink target changed")
			}
		})
	}
}
func TestBundledMacDefaultOfflineAndEveryResolution(t *testing.T) {
	if runtime.GOOS != "darwin" || runtime.GOARCH != "arm64" {
		t.Skip("Mac arm64 only")
	}
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("PATH", t.TempDir())
	p, e := (Cloudflared{}).executable(context.Background())
	if e != nil {
		t.Fatal(e)
	}
	out, e := exec.Command(p, "--version").CombinedOutput()
	if e != nil || !strings.Contains(string(out), "2026.9.0") {
		t.Fatalf("offline version: %v %s", e, out)
	}
	p2, e := (Cloudflared{}).executable(context.Background())
	if e != nil || p2 != p {
		t.Fatalf("default reuse: %v", e)
	}
	if e = os.Chmod(p, 0755); e != nil {
		t.Fatal(e)
	}
	if _, e = (Cloudflared{}).executable(context.Background()); e == nil {
		t.Fatal("default selector skipped per-resolution verification")
	}
}
func TestAndroidProviderRejectsUninstalledAndTraversalPaths(t *testing.T) {
	for _, path := range []string{"relative", "/data/local/tmp", "/data/user/0/app/files", "/data/app/test/../lib", "/data/app/test/./lib", "/mnt/expand/id/user/app"} {
		if _, e := installedAndroidPath(path); e == nil {
			t.Fatalf("accepted %s", path)
		}
	}
}
