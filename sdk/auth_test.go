package cybergrid

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

type oauthFixture struct {
	server *httptest.Server
	config AuthConfig
	mu     sync.Mutex
	polls  []time.Time
	grant  func(http.ResponseWriter, *http.Request, int)
}

func newOAuthFixture(t *testing.T) (*Client, *oauthFixture) {
	t.Helper()
	f := &oauthFixture{}
	f.server = httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		if r.URL.Path == "/api/v1/auth/config" {
			if r.URL.Query().Get("appId") != "native-public" {
				t.Error("explicit case-sensitive app query missing")
			}
			_ = json.NewEncoder(w).Encode(f.config)
			return
		}
		if r.Header.Get("Authorization") != "" {
			t.Error("central bearer sent to OAuth endpoint")
		}
		if err := r.ParseForm(); err != nil {
			t.Error(err)
		}
		if r.Form.Get("client_id") != "native-public" || r.Form.Get("client_secret") != "" {
			t.Error("incorrect public client form")
		}
		switch r.URL.Path {
		case "/oidc/device/auth":
			if r.Form.Get("resource") != f.config.Resource || r.Form.Get("scope") != f.config.Scope {
				t.Error("resource/scope missing")
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"device_code": "private-device-code", "user_code": "ABCD-EFGH", "verification_uri": f.server.URL + "/device", "verification_uri_complete": f.server.URL + "/device?user_code=ABCD-EFGH", "expires_in": 60, "interval": 1})
		case "/oidc/token":
			f.mu.Lock()
			f.polls = append(f.polls, time.Now())
			n := len(f.polls)
			f.mu.Unlock()
			if f.grant != nil {
				f.grant(w, r, n)
				return
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "resource-jwt", "refresh_token": "rotated-refresh", "expires_in": 300, "token_type": "Bearer", "scope": "connect"})
		case "/oidc/revoke":
			if r.Form.Get("token") != "refresh-secret" {
				t.Error("bad revocation request")
			}
			w.WriteHeader(http.StatusOK)
		default:
			w.WriteHeader(404)
		}
	}))
	t.Cleanup(f.server.Close)
	f.config = AuthConfig{Provider: "logto", Issuer: f.server.URL + "/oidc", ClientID: "native-public", Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access profile email connect", DeviceAuthorizationEndpoint: f.server.URL + "/oidc/device/auth", TokenEndpoint: f.server.URL + "/oidc/token", RevocationEndpoint: f.server.URL + "/oidc/revoke"}
	c, err := New(Config{BaseURL: f.server.URL, AppID: "native-public", HTTPClient: f.server.Client(), DisableTunnel: true})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { c.Close() })
	return c, f
}

func TestDeviceAuthorizationPendingAndResourceRefresh(t *testing.T) {
	c, f := newOAuthFixture(t)
	refreshCount := 0
	expectedRefresh := "initial-device-refresh"
	var grantMu sync.Mutex
	f.grant = func(w http.ResponseWriter, r *http.Request, n int) {
		grantMu.Lock()
		defer grantMu.Unlock()
		if r.Form.Get("grant_type") == "refresh_token" {
			if r.Form.Get("resource") != f.config.Resource || r.Form.Get("refresh_token") != expectedRefresh {
				t.Error("refresh binding or rotation missing")
			}
			refreshCount++
			expectedRefresh = fmt.Sprintf("resource-refresh-%d", refreshCount)
			_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "resource-jwt", "refresh_token": expectedRefresh, "expires_in": 300, "token_type": "Bearer", "scope": "connect"})
			return
		}
		if r.Form.Get("device_code") != "private-device-code" {
			t.Error("incorrect device code")
		}
		if n == 1 {
			w.WriteHeader(400)
			fmt.Fprint(w, `{"error":"authorization_pending"}`)
			return
		}
		// Actual Logto returns an OIDC-only opaque token for this grant,
		// even when the initial device authorization includes resource.
		_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "opaque-oidc-only", "refresh_token": "initial-device-refresh", "expires_in": 300, "token_type": "Bearer", "scope": "openid profile email offline_access"})
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	auth, err := c.BeginDeviceAuthorization(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if auth.Interval != 5 {
		t.Fatal("interval floor not enforced")
	}
	encoded, _ := json.Marshal(auth)
	if strings.Contains(string(encoded), "private-device-code") || strings.Contains(fmt.Sprintf("%#v", auth), "private-device-code") {
		t.Fatal("device secret leaked")
	}
	tokens, err := c.CompleteDeviceAuthorization(ctx, auth)
	if err != nil {
		t.Fatal(err)
	}
	if tokens.AccessToken != "resource-jwt" || tokens.RefreshToken != "resource-refresh-1" {
		t.Fatal("Complete must obtain the API resource token and latest rotated refresh token")
	}
	f.mu.Lock()
	gap := f.polls[1].Sub(f.polls[0])
	f.mu.Unlock()
	if gap < 5*time.Second {
		t.Fatal("polled too fast")
	}
	if err = c.AcknowledgePendingGrant(tokens.RefreshToken); err != nil {
		t.Fatal(err)
	}
	fresh, err := c.RefreshAccessToken(ctx, tokens.RefreshToken)
	if err != nil {
		t.Fatal(err)
	}
	if fresh.RefreshToken != "resource-refresh-2" {
		t.Fatal("rotated refresh discarded")
	}
	b, _ := json.Marshal(fresh)
	if strings.Contains(string(b), "resource-jwt") || strings.Contains(fmt.Sprintf("%+v", fresh), "resource-refresh-2") {
		t.Fatal("token leaked")
	}
	if err = c.RevokeRefreshToken(ctx, "refresh-secret"); err != nil {
		t.Fatal(err)
	}
}

func TestDeviceAuthorizationRejectsMissingRefreshAndResourceFailure(t *testing.T) {
	for _, scenario := range []string{"missing-refresh", "resource-denied", "rotation-optional"} {
		t.Run(scenario, func(t *testing.T) {
			c, f := newOAuthFixture(t)
			f.grant = func(w http.ResponseWriter, r *http.Request, n int) {
				if r.Form.Get("grant_type") == "refresh_token" {
					if r.Form.Get("resource") != f.config.Resource || r.Form.Get("refresh_token") != "device-refresh" {
						t.Error("resource refresh omitted required binding")
					}
					if scenario == "resource-denied" {
						w.WriteHeader(400)
						fmt.Fprint(w, `{"error":"invalid_scope"}`)
						return
					}
					_ = json.NewEncoder(w).Encode(map[string]any{"access_token": "api-resource-token", "expires_in": 300, "token_type": "Bearer"})
					return
				}
				response := map[string]any{"access_token": "opaque-oidc-only", "expires_in": 300, "token_type": "Bearer"}
				if scenario != "missing-refresh" {
					response["refresh_token"] = "device-refresh"
				}
				_ = json.NewEncoder(w).Encode(response)
			}
			ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
			defer cancel()
			auth, err := c.BeginDeviceAuthorization(ctx)
			if err != nil {
				t.Fatal(err)
			}
			tokens, err := c.CompleteDeviceAuthorization(ctx, auth)
			if scenario == "rotation-optional" {
				if err != nil || tokens.AccessToken != "api-resource-token" || tokens.RefreshToken != "device-refresh" {
					t.Fatalf("current refresh token must survive a resource response without rotation: %v", err)
				}
				return
			}
			if err == nil || tokens.AccessToken != "" || tokens.RefreshToken != "" {
				t.Fatal("failed resource authorization returned the initial opaque token")
			}
			if scenario == "missing-refresh" && !strings.Contains(err.Error(), "offline refresh token") {
				t.Fatal("missing refresh token not explained")
			}
			if scenario == "resource-denied" {
				var oe *OAuthError
				if !errors.As(err, &oe) || oe.Code != "invalid_scope" {
					t.Fatalf("resource denial was hidden: %v", err)
				}
			}
		})
	}
}

func TestDeviceAuthorizationSlowDownCancellationAndBinding(t *testing.T) {
	c, f := newOAuthFixture(t)
	f.grant = func(w http.ResponseWriter, r *http.Request, n int) {
		w.WriteHeader(400)
		fmt.Fprint(w, `{"error":"slow_down","error_description":"do not log secrets"}`)
	}
	auth, err := c.BeginDeviceAuthorization(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	other, _ := New(Config{BaseURL: f.server.URL, AppID: "other"})
	if _, err = other.CompleteDeviceAuthorization(context.Background(), auth); err == nil {
		t.Fatal("cross-client device code accepted")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 7*time.Second)
	defer cancel()
	_, err = c.CompleteDeviceAuthorization(ctx, auth)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("expected cancellation, got %v", err)
	}
	f.mu.Lock()
	n := len(f.polls)
	f.mu.Unlock()
	if n != 1 {
		t.Fatalf("slow_down must delay second poll ten seconds, saw %d", n)
	}
	auth.expiresAt = time.Now().Add(-time.Second)
	if _, err = c.CompleteDeviceAuthorization(context.Background(), auth); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatal("expired code polled")
	}
}

func TestOAuthConfigurationAndErrorsFailClosed(t *testing.T) {
	c, f := newOAuthFixture(t)
	for _, mutate := range []func(*AuthConfig){
		func(a *AuthConfig) { a.Provider = "unknown" },
		func(a *AuthConfig) { a.TokenEndpoint = "https://other.example/token" },
		func(a *AuthConfig) { a.TokenEndpoint = "" },
		func(a *AuthConfig) { a.DeviceAuthorizationEndpoint = "http://localhost/device" },
		func(a *AuthConfig) { a.Scope = "openid offline_access" },
	} {
		a := f.config
		mutate(&a)
		if validateAuthConfig(a) == nil {
			t.Fatalf("accepted invalid config %+v", a)
		}
	}
	if _, err := c.AuthConfig(context.Background()); err != nil {
		t.Fatal(err)
	}
	f.config.ClientID = "changed"
	if _, err := c.AuthConfig(context.Background()); err == nil {
		t.Fatal("accepted config change mid-lifecycle")
	}
	if _, _, err := c.Register(context.Background(), "unused", "unused"); err == nil {
		t.Fatal("legacy registration accepted")
	}
	if _, err := c.Login(context.Background(), "unused", "unused", "unused"); err == nil {
		t.Fatal("legacy login accepted")
	}
}

func TestOAuthDenialLimitsAndSecretRedaction(t *testing.T) {
	c, f := newOAuthFixture(t)
	for _, code := range []string{"access_denied", "expired_token", "invalid_grant", "unsupported_grant_type", "unrecognized-private-token"} {
		f.grant = func(w http.ResponseWriter, r *http.Request, n int) {
			w.WriteHeader(400)
			_ = json.NewEncoder(w).Encode(map[string]string{"error": code, "error_description": "private-provider-details"})
		}
		_, err := c.RefreshAccessToken(context.Background(), "refresh-secret")
		var oe *OAuthError
		if !errors.As(err, &oe) || oe.Code != safeOAuthError(code) {
			t.Fatalf("wrong OAuth error: %v", err)
		}
		if strings.Contains(err.Error(), "private") {
			t.Fatal("untrusted error detail leaked")
		}
	}
	f.grant = func(w http.ResponseWriter, r *http.Request, n int) {
		_, _ = w.Write([]byte(strings.Repeat("x", (64<<10)+1)))
	}
	if _, err := c.RefreshAccessToken(context.Background(), "refresh-secret"); err == nil || !strings.Contains(err.Error(), "64 KiB") {
		t.Fatal("oversized OAuth response accepted")
	}
}
