package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	cybergrid "github.com/ywateru/CyberGrid/sdk"
	"golang.org/x/sys/unix"
)

// A credential file contains one device's grant, never an account credential or
// reusable CyberGrid session. Do not copy it to other devices: authorize each.
type credentials struct {
	Version                     int    `json:"version"`
	AppID                       string `json:"appId"`
	Server                      string `json:"server"`
	Issuer                      string `json:"issuer"`
	ClientID                    string `json:"clientId"`
	Resource                    string `json:"resource"`
	RefreshToken                string `json:"refreshToken"`
	Scope                       string `json:"scope"`
	DeviceAuthorizationEndpoint string `json:"deviceAuthorizationEndpoint"`
	TokenEndpoint               string `json:"tokenEndpoint"`
	RevocationEndpoint          string `json:"revocationEndpoint"`
}

func (c credentials) String() string   { return "credentials{redacted}" }
func (c credentials) GoString() string { return c.String() }

func newCredentials(server string, a cybergrid.AuthConfig, refresh string) credentials {
	return credentials{Version: 4, AppID: a.ClientID, Server: strings.TrimRight(server, "/"), Issuer: a.Issuer, ClientID: a.ClientID, Resource: a.Resource, Scope: a.Scope, DeviceAuthorizationEndpoint: a.DeviceAuthorizationEndpoint, TokenEndpoint: a.TokenEndpoint, RevocationEndpoint: a.RevocationEndpoint, RefreshToken: refresh}
}

func (c credentials) validateBinding(server, appID string) error {
	if c.Version != 4 || c.RefreshToken == "" {
		return errors.New("version 4 Logto credentials required; authorize independently into a new file; revoke old grants before API upgrade or ask the Logto maintainer after cutover")
	}
	if c.Server != strings.TrimRight(server, "/") || c.AppID != appID || c.ClientID != appID {
		return errors.New("credential server/appId/clientId binding differs from this command; existing credentials are unchanged")
	}
	return nil
}

func (c credentials) validate(server, appID string, a cybergrid.AuthConfig) error {
	if err := c.validateBinding(server, appID); err != nil {
		return err
	}
	if c.Issuer != a.Issuer || c.ClientID != a.ClientID || c.Resource != a.Resource || c.Scope != a.Scope || c.DeviceAuthorizationEndpoint != a.DeviceAuthorizationEndpoint || c.TokenEndpoint != a.TokenEndpoint || c.RevocationEndpoint != a.RevocationEndpoint {
		return errors.New("credential OAuth namespace/endpoints binding differs from live configuration; use a separate file and login")
	}
	return nil
}

// The lock spans read -> refresh -> atomic write. Never delete lock files: doing
// so could give two concurrent processes distinct locks for the same grant.
func lockCredentials(ctx context.Context, path string) (func(), error) {
	if path == "" {
		return nil, errors.New("-credentials path (or CYBERGRID_CREDENTIALS) is required")
	}
	fd, err := unix.Open(path+".lock", unix.O_CREAT|unix.O_RDWR|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0600)
	if err != nil {
		return nil, fmt.Errorf("open credential lock: %w", err)
	}
	f := os.NewFile(uintptr(fd), path+".lock")
	if info, err := f.Stat(); err != nil || !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 {
		f.Close()
		return nil, errors.New("credential lock must be a regular private file (0600)")
	}
	for {
		err = unix.Flock(fd, unix.LOCK_EX|unix.LOCK_NB)
		if err == nil {
			return func() { _ = unix.Flock(fd, unix.LOCK_UN); _ = f.Close() }, nil
		}
		if err != unix.EWOULDBLOCK && err != unix.EAGAIN {
			f.Close()
			return nil, err
		}
		timer := time.NewTimer(100 * time.Millisecond)
		select {
		case <-ctx.Done():
			timer.Stop()
			f.Close()
			return nil, ctx.Err()
		case <-timer.C:
		}
	}
}

func readCredentials(path string) (credentials, error) {
	var c credentials
	fd, err := unix.Open(path, unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
	if err != nil {
		return c, fmt.Errorf("open credentials (run login first): %w", err)
	}
	f := os.NewFile(uintptr(fd), path)
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return c, err
	}
	if !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 {
		return c, errors.New("credentials must be a regular private file (chmod 600)")
	}
	b, err := io.ReadAll(io.LimitReader(f, (64<<10)+1))
	if err != nil {
		return c, err
	}
	if len(b) > 64<<10 {
		return c, errors.New("credential file exceeds 64 KiB")
	}
	if err = json.Unmarshal(b, &c); err != nil {
		return c, errors.New("invalid credential file JSON")
	}
	return c, nil
}

func writeCredentials(path string, c credentials) error {
	if c.Version != 4 || c.RefreshToken == "" || c.AppID == "" || c.AppID != c.ClientID || c.Scope == "" || c.DeviceAuthorizationEndpoint == "" || c.TokenEndpoint == "" {
		return errors.New("cannot save incomplete Logto credentials")
	}
	// Refuse replacing a symlink or permissive credential file, even on login.
	if info, err := os.Lstat(path); err == nil {
		if !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 {
			return errors.New("existing credential path must be a regular private file (0600)")
		}
		existing, err := readCredentials(path)
		if err != nil {
			return err
		}
		if existing.Version != 4 || existing.Server != c.Server || existing.AppID != c.AppID || existing.ClientID != c.ClientID || existing.Issuer != c.Issuer || existing.Resource != c.Resource || existing.Scope != c.Scope || existing.DeviceAuthorizationEndpoint != c.DeviceAuthorizationEndpoint || existing.TokenEndpoint != c.TokenEndpoint || existing.RevocationEndpoint != c.RevocationEndpoint {
			return errors.New("refusing to overwrite old or differently bound credentials")
		}
	} else if !os.IsNotExist(err) {
		return err
	}
	f, err := os.CreateTemp(filepath.Dir(path), ".cybergrid-auth-*")
	if err != nil {
		return err
	}
	defer os.Remove(f.Name())
	if err = json.NewEncoder(f).Encode(c); err == nil {
		err = f.Sync()
	}
	closeErr := f.Close()
	if err != nil {
		return err
	}
	if closeErr != nil {
		return closeErr
	}
	if err = os.Rename(f.Name(), path); err != nil {
		return err
	}
	return nil
}

// A failed write leaves the private file lease with the pending-grant owner.
// Retrying its checkpoint/revocation reuses that lease; another process cannot
// read and rotate stale bytes while this process still owns a newer grant.
type credentialLease struct {
	gate   sync.Mutex
	path   string
	unlock func()
}

var credentialLeasesMu sync.Mutex
var credentialLeases = map[*cybergrid.Client]*credentialLease{}

func leaseFor(c *cybergrid.Client) *credentialLease {
	credentialLeasesMu.Lock()
	defer credentialLeasesMu.Unlock()
	l := credentialLeases[c]
	if l == nil {
		l = &credentialLease{}
		credentialLeases[c] = l
	}
	return l
}
func acquireCredentialLease(ctx context.Context, c *cybergrid.Client, path string) (func(), error) {
	l := leaseFor(c)
	l.gate.Lock()
	if l.unlock != nil && l.path != path {
		l.gate.Unlock()
		return nil, errors.New("pending grant owns another private credential file")
	}
	if l.unlock == nil {
		unlock, err := lockCredentials(ctx, path)
		if err != nil {
			l.gate.Unlock()
			return nil, err
		}
		l.path = path
		l.unlock = unlock
	}
	return func() {
		if !c.HasPendingGrant() && l.unlock != nil {
			l.unlock()
			l.unlock = nil
		}
		l.gate.Unlock()
	}, nil
}

// OAuth refresh rotation is committed before any exchange or data connection.
func refreshCredentials(ctx context.Context, c *cybergrid.Client, server, appID, path string) (cybergrid.OAuthTokens, error) {
	release, err := acquireCredentialLease(ctx, c, path)
	if err != nil {
		return cybergrid.OAuthTokens{}, err
	}
	defer release()
	c.SetGrantCheckpoint(func(saveCtx context.Context, client *cybergrid.Client, t cybergrid.OAuthTokens) error {
		a, ok := client.CachedAuthConfig()
		if !ok {
			return errors.New("verified auth binding missing")
		}
		return writeCredentials(path, newCredentials(server, a, t.RefreshToken))
	})
	if c.HasPendingGrant() {
		if err = c.RetryPendingGrant(ctx); err != nil {
			return cybergrid.OAuthTokens{}, err
		}
	}
	saved, err := readCredentials(path)
	if err != nil {
		return cybergrid.OAuthTokens{}, err
	}
	if err = saved.validateBinding(server, appID); err != nil {
		return cybergrid.OAuthTokens{}, err
	}
	a, err := c.AuthConfig(ctx)
	if err != nil {
		return cybergrid.OAuthTokens{}, err
	}
	if err = saved.validate(server, appID, a); err != nil {
		return cybergrid.OAuthTokens{}, err
	}
	tokens, err := c.RefreshAccessToken(ctx, saved.RefreshToken)
	if err != nil {
		return cybergrid.OAuthTokens{}, err
	}
	if tokens.RefreshToken != "" {
		saved.RefreshToken = tokens.RefreshToken
	}
	if err = writeCredentials(path, saved); err != nil {
		return cybergrid.OAuthTokens{}, fmt.Errorf("cannot persist rotated refresh credentials: %w", err)
	}
	return tokens, nil
}
