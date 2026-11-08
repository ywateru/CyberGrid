package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"time"

	cybergrid "github.com/ywateru/CyberGrid/sdk"
)

func envDefault(key, fallback string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return fallback
}

func login(ctx context.Context, c *cybergrid.Client, server, appID, path string, openBrowser bool) error {
	if openBrowser && runtime.GOOS != "darwin" {
		return errors.New("-open-browser is available on macOS; open the displayed verification URL on another trusted device")
	}
	unlock, err := acquireCredentialLease(ctx, c, path)
	if err != nil {
		return err
	}
	defer unlock()
	if _, err = os.Lstat(path); err == nil {
		return errors.New("credential file already exists; use it, or logout before authorizing a replacement grant")
	} else if !os.IsNotExist(err) {
		return err
	}
	auth, err := c.BeginDeviceAuthorization(ctx)
	if err != nil {
		return err
	}
	output(map[string]any{"event": "device-authorization", "verificationUri": auth.VerificationURI, "verificationUriComplete": auth.VerificationURIComplete, "userCode": auth.UserCode, "expiresIn": auth.ExpiresIn, "instruction": "Open the verification URL in a trusted browser, confirm this device code, and complete Logto sign-in or registration. Never approve an unsolicited code."})
	if openBrowser {
		url := auth.VerificationURIComplete
		if url == "" {
			url = auth.VerificationURI
		}
		if err = exec.CommandContext(ctx, "open", url).Run(); err != nil {
			return fmt.Errorf("open browser: %w", err)
		}
	}
	tokens, err := c.CompleteDeviceAuthorization(ctx, auth)
	if err != nil {
		return err
	}
	if tokens.RefreshToken == "" {
		return errors.New("Logto did not return an offline refresh token; verify native app offline_access configuration")
	}
	a, ok := c.CachedAuthConfig()
	if !ok {
		return errors.New("verified OAuth binding unavailable; pending grant retained")
	}
	if err = writeCredentials(path, newCredentials(server, a, tokens.RefreshToken)); err != nil {
		return err
	}
	output(map[string]any{"event": "authorized", "credentials": path, "version": 4, "appId": appID, "message": "Refresh grant saved privately. Each command creates a fresh CyberGrid device session."})
	return nil
}

func logout(ctx context.Context, c *cybergrid.Client, server, appID, path string) error {
	unlock, err := lockCredentials(ctx, path)
	if err != nil {
		return err
	}
	defer unlock()
	saved, err := readCredentials(path)
	if err != nil {
		return err
	}
	// Refuse old or cross-application files before any provider request. Never
	// delete or overwrite a file whose authorization binding is not verified.
	if err = saved.validateBinding(server, appID); err != nil {
		return err
	}
	a, err := c.AuthConfig(ctx)
	if err != nil {
		return err
	}
	if err = saved.validate(server, appID, a); err != nil {
		return err
	}
	if err = c.RevokeRefreshToken(ctx, saved.RefreshToken); err != nil {
		return fmt.Errorf("refresh grant revocation failed; local credentials preserved: %w", err)
	}
	if err = os.Remove(path); err != nil {
		return fmt.Errorf("refresh grant revoked but local credential deletion failed: %w", err)
	}
	output(map[string]any{"event": "logout", "localCredentialsDeleted": true, "refreshGrantRevoked": true, "message": "Existing CyberGrid sessions and browser sign-ins are separate; stop running SDK processes to close their sessions."})
	return nil
}

// Never silently drop a live uncommitted refresh rotation on command failure.
func preservePending(c *cybergrid.Client) {
	l := leaseFor(c)
	l.gate.Lock()
	defer l.gate.Unlock()
	defer func() {
		if !c.HasPendingGrant() && l.unlock != nil {
			l.unlock()
			l.unlock = nil
		}
	}()
	for c.HasPendingGrant() {
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		err := c.RetryPendingGrant(ctx)
		cancel()
		if err == nil {
			return
		}
		ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
		err = c.RevokePendingGrant(ctx)
		cancel()
		if err == nil {
			return
		}
		fmt.Fprintln(os.Stderr, "Latest refresh grant remains private in this process; retrying save or revocation. Do not terminate until recovery completes.")
		time.Sleep(5 * time.Second)
	}
}
