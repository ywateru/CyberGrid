package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	cybergrid "github.com/ywateru/CyberGrid/sdk"
)

func TestConcurrentRefreshRotationIsSerialized(t *testing.T) {
	var mu sync.Mutex
	current := "initial-refresh"
	count := 0
	var config cybergrid.AuthConfig
	s := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		if r.URL.Path == "/api/v1/auth/config" {
			if r.URL.Query().Get("appId") != "native" {
				t.Error("app query missing")
			}
			_ = json.NewEncoder(w).Encode(config)
			return
		}
		mu.Lock()
		defer mu.Unlock()
		_ = r.ParseForm()
		if r.Form.Get("refresh_token") != current {
			w.WriteHeader(400)
			fmt.Fprint(w, `{"error":"invalid_grant"}`)
			return
		}
		count++
		current = fmt.Sprintf("rotation-%d", count)
		_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "resource-access", "refresh_token": current, "expires_in": 300, "token_type": "Bearer"})
	}))
	defer s.Close()
	config = cybergrid.AuthConfig{Provider: "logto", Issuer: s.URL + "/oidc", ClientID: "native", Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: s.URL + "/device", TokenEndpoint: s.URL + "/token"}
	path := filepath.Join(t.TempDir(), "credentials.json")
	if err := writeCredentials(path, newCredentials(s.URL, config, current)); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	errorsOut := make(chan error, 2)
	for range 2 {
		go func() {
			c, err := cybergrid.New(cybergrid.Config{BaseURL: s.URL, AppID: "native", HTTPClient: s.Client(), DisableTunnel: true})
			if err == nil {
				defer c.Close()
				_, err = refreshCredentials(ctx, c, s.URL, "native", path)
			}
			errorsOut <- err
		}()
	}
	for range 2 {
		if err := <-errorsOut; err != nil {
			t.Fatal(err)
		}
	}
	saved, err := readCredentials(path)
	if err != nil {
		t.Fatal(err)
	}
	if saved.RefreshToken != "rotation-2" {
		t.Fatal("latest rotated grant was lost")
	}
	info, _ := os.Stat(path)
	if info.Mode().Perm() != 0600 {
		t.Fatal("credential mode is not 0600")
	}
	// A different central URL must never receive or refresh this grant.
	c, _ := cybergrid.New(cybergrid.Config{BaseURL: s.URL, AppID: "native", HTTPClient: s.Client()})
	defer c.Close()
	if _, err = refreshCredentials(ctx, c, "https://different.example", "native", path); err == nil {
		t.Fatal("cross-server credential reuse accepted")
	}
	mu.Lock()
	defer mu.Unlock()
	if count != 2 {
		t.Fatal("mismatched grant sent to OAuth provider")
	}
}

func TestOldAndCrossAppCredentialsAreUnchangedBeforeRefreshOrLogout(t *testing.T) {
	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { requests.Add(1); w.WriteHeader(500) }))
	defer server.Close()
	a := cybergrid.AuthConfig{ClientID: "NativeA", Issuer: "https://id.example/oidc", Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: "https://id.example/device", TokenEndpoint: "https://id.example/token"}
	for _, tc := range []struct {
		name   string
		mutate func(*credentials)
	}{
		{"v3", func(c *credentials) { c.Version = 3 }},
		{"v2", func(c *credentials) { c.Version = 2; c.AppID = "" }},
		{"cross-app", func(c *credentials) { c.AppID = "NativeB"; c.ClientID = "NativeB" }},
		{"client-mismatch", func(c *credentials) { c.ClientID = "nativea" }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "auth.json")
			saved := newCredentials(server.URL, a, "test-refresh-grant")
			tc.mutate(&saved)
			before, err := json.Marshal(saved)
			if err != nil {
				t.Fatal(err)
			}
			if err = os.WriteFile(path, before, 0600); err != nil {
				t.Fatal(err)
			}
			c, err := cybergrid.New(cybergrid.Config{BaseURL: server.URL, AppID: "NativeA", DisableTunnel: true})
			if err != nil {
				t.Fatal(err)
			}
			defer c.Close()
			if _, err = refreshCredentials(context.Background(), c, server.URL, "NativeA", path); err == nil {
				t.Fatal("old or mismatched grant was refreshed")
			}
			if err = logout(context.Background(), c, server.URL, "NativeA", path); err == nil {
				t.Fatal("old or mismatched grant was revoked/deleted")
			}
			if err = writeCredentials(path, newCredentials(server.URL, a, "replacement-grant")); err == nil {
				t.Fatal("old or differently bound file was overwritten")
			}
			after, err := os.ReadFile(path)
			if err != nil || string(after) != string(before) {
				t.Fatal("rejected grant file changed or was deleted")
			}
			if requests.Load() != 0 {
				t.Fatal("rejected credentials caused a network request")
			}
		})
	}
}

func TestLogoutPreservesV4CredentialsOnProviderFailure(t *testing.T) {
	var config cybergrid.AuthConfig
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/v1/auth/config" {
			_ = json.NewEncoder(w).Encode(config)
			return
		}
		w.WriteHeader(503)
	}))
	defer server.Close()
	config = cybergrid.AuthConfig{Provider: "logto", Issuer: server.URL + "/oidc", ClientID: "NativeA", Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: server.URL + "/device", TokenEndpoint: server.URL + "/token", RevocationEndpoint: server.URL + "/revoke"}
	path := filepath.Join(t.TempDir(), "auth.json")
	if err := writeCredentials(path, newCredentials(server.URL, config, "test-refresh")); err != nil {
		t.Fatal(err)
	}
	before, _ := os.ReadFile(path)
	c, err := cybergrid.New(cybergrid.Config{BaseURL: server.URL, AppID: "NativeA", HTTPClient: server.Client()})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	if err = logout(context.Background(), c, server.URL, "NativeA", path); err == nil {
		t.Fatal("failed revocation claimed success")
	}
	after, err := os.ReadFile(path)
	if err != nil || string(after) != string(before) {
		t.Fatal("failed revocation deleted credentials")
	}
}

func TestCredentialPermissionSymlinkAndLockCancellation(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "auth.json")
	config := cybergrid.AuthConfig{Issuer: "https://id.example/oidc", ClientID: "native", Resource: "https://api.example", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: "https://id.example/device", TokenEndpoint: "https://id.example/token"}
	c := newCredentials("https://api.example", config, "private-refresh")
	if err := writeCredentials(path, c); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(path, 0644); err != nil {
		t.Fatal(err)
	}
	if _, err := readCredentials(path); err == nil {
		t.Fatal("public credential file accepted")
	}
	if err := writeCredentials(path, c); err == nil {
		t.Fatal("public credential overwritten without warning")
	}
	if err := os.Chmod(path, 0600); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(dir, "link")
	if err := os.Symlink(path, link); err != nil {
		t.Fatal(err)
	}
	if _, err := readCredentials(link); err == nil {
		t.Fatal("symlink credential accepted")
	}
	if err := writeCredentials(link, c); err == nil {
		t.Fatal("symlink credential overwritten")
	}
	unlock, err := lockCredentials(context.Background(), path)
	if err != nil {
		t.Fatal(err)
	}
	defer unlock()
	ctx, cancel := context.WithTimeout(context.Background(), 150*time.Millisecond)
	defer cancel()
	if _, err = lockCredentials(ctx, path); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatal("contending lock ignored cancellation")
	}
	old := c
	old.Version = 1
	if err = old.validate("https://api.example", "native", config); err == nil {
		t.Fatal("legacy credential schema accepted")
	}
	for _, mutate := range []func(*credentials){func(c *credentials) { c.Issuer = "other" }, func(c *credentials) { c.ClientID = "other" }, func(c *credentials) { c.Resource = "other" }} {
		copy := c
		mutate(&copy)
		if copy.validate("https://api.example", "native", config) == nil {
			t.Fatal("credential binding mismatch accepted")
		}
	}
}

func TestV4BindsOAuthEndpointsAcrossNewClientsBeforeRefresh(t *testing.T) {
	var tokens atomic.Int32
	var config cybergrid.AuthConfig
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/v1/auth/config" {
			json.NewEncoder(w).Encode(config)
			return
		}
		tokens.Add(1)
		w.WriteHeader(500)
	}))
	defer server.Close()
	original := cybergrid.AuthConfig{Provider: "logto", Issuer: server.URL + "/oidc", ClientID: "NativeA", Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: server.URL + "/device", TokenEndpoint: server.URL + "/token", RevocationEndpoint: server.URL + "/revoke"}
	for _, field := range []string{"scope", "device", "token", "revoke"} {
		t.Run(field, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "v4.json")
			if err := writeCredentials(path, newCredentials(server.URL, original, "private-refresh")); err != nil {
				t.Fatal(err)
			}
			before, _ := os.ReadFile(path)
			config = original
			switch field {
			case "scope":
				config.Scope += " profile"
			case "device":
				config.DeviceAuthorizationEndpoint += "-changed"
			case "token":
				config.TokenEndpoint += "-changed"
			case "revoke":
				config.RevocationEndpoint += "-changed"
			}
			for range 2 {
				c, err := cybergrid.New(cybergrid.Config{BaseURL: server.URL, AppID: "NativeA", HTTPClient: server.Client()})
				if err != nil {
					t.Fatal(err)
				}
				if _, err = refreshCredentials(context.Background(), c, server.URL, "NativeA", path); err == nil {
					t.Fatal("new Client forwarded old refresh under changed OAuth configuration")
				}
				c.Close()
			}
			after, _ := os.ReadFile(path)
			if string(before) != string(after) || tokens.Load() != 0 {
				t.Fatal("rejected private binding was modified or sent to provider")
			}
		})
	}
}

func TestCredentialLeaseChild(t *testing.T) {
	path := os.Getenv("CYBERGRID_TEST_LOCK_PATH")
	if path == "" {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancel()
	unlock, err := lockCredentials(ctx, path)
	if err != nil {
		fmt.Print("LEASE_BUSY")
		return
	}
	defer unlock()
	if _, err = readCredentials(path); err != nil {
		t.Fatal(err)
	}
	fmt.Print("LEASE_READ_ALLOWED")
}
func TestFailedRotationOwnsFileLeaseAcrossProcessesUntilCheckpoint(t *testing.T) {
	var config cybergrid.AuthConfig
	path := filepath.Join(t.TempDir(), "v4.json")
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/v1/auth/config" {
			json.NewEncoder(w).Encode(config)
			return
		}
		if err := os.Chmod(path, 0644); err != nil {
			t.Error(err)
		}
		json.NewEncoder(w).Encode(map[string]any{"access_token": "resource", "refresh_token": "newest-private-rotation", "expires_in": 300, "token_type": "Bearer"})
	}))
	defer server.Close()
	config = cybergrid.AuthConfig{Provider: "logto", Issuer: server.URL + "/oidc", ClientID: "NativeA", Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: server.URL + "/device", TokenEndpoint: server.URL + "/token"}
	if err := writeCredentials(path, newCredentials(server.URL, config, "old-private-grant")); err != nil {
		t.Fatal(err)
	}
	c, err := cybergrid.New(cybergrid.Config{BaseURL: server.URL, AppID: "NativeA", HTTPClient: server.Client()})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	if _, err = refreshCredentials(context.Background(), c, server.URL, "NativeA", path); err == nil || !c.HasPendingGrant() {
		t.Fatal("rotation failure did not preserve pending owner")
	}
	child := func(want string) {
		cmd := exec.Command(os.Args[0], "-test.run=^TestCredentialLeaseChild$")
		cmd.Env = append(os.Environ(), "CYBERGRID_TEST_LOCK_PATH="+path)
		out, err := cmd.CombinedOutput()
		if err != nil || !strings.Contains(string(out), want) {
			t.Fatalf("child lease: %s %v", out, err)
		}
	}
	child("LEASE_BUSY")
	if err = os.Chmod(path, 0600); err != nil {
		t.Fatal(err)
	}
	// Product exit cleanup resolves the pending grant under its retained lease.
	preservePending(c)
	l := leaseFor(c)
	l.gate.Lock()
	retained := l.unlock != nil
	l.gate.Unlock()
	if retained || c.HasPendingGrant() {
		t.Fatal("resolved checkpoint kept stale lease")
	}
	child("LEASE_READ_ALLOWED")
	saved, err := readCredentials(path)
	if err != nil || saved.RefreshToken != "newest-private-rotation" {
		t.Fatal("resolved lease did not preserve newest rotation")
	}
}
