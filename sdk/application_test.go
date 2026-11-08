package cybergrid

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestAuthConfigUsesEncodedCaseSensitiveApplicationID(t *testing.T) {
	for _, id := range []string{"NativeCaseA", "Native&Case+?A"} {
		t.Run(id, func(t *testing.T) {
			var origin string
			expectedQuery := url.Values{"appId": {id}}.Encode()
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path != "/api/v1/auth/config" || r.URL.Query().Get("appId") != id || r.URL.RawQuery != expectedQuery {
					t.Error("application query not encoded exactly or case was changed")
				}
				_ = json.NewEncoder(w).Encode(AuthConfig{Provider: "logto", Issuer: origin + "/oidc", ClientID: id, Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: origin + "/device", TokenEndpoint: origin + "/token"})
			}))
			defer server.Close()
			origin = server.URL
			c, err := New(Config{BaseURL: server.URL, AppID: id, HTTPClient: server.Client()})
			if err != nil {
				t.Fatal(err)
			}
			a, err := c.AuthConfig(context.Background())
			if err != nil || a.ClientID != id {
				t.Fatalf("configuration binding failed: %v", err)
			}
		})
	}
	for _, id := range []string{"", strings.Repeat("A", 22)} {
		if _, err := New(Config{BaseURL: "https://cybergrid.aixgg.com", AppID: id}); err == nil {
			t.Fatal("missing/oversized app ID accepted")
		}
	}
	if _, err := New(Config{BaseURL: "https://cybergrid.aixgg.com", AppID: strings.Repeat("A", 21)}); err != nil {
		t.Fatal("valid 21-character app ID rejected")
	}
}

func TestAuthConfigRejectsDifferentClientID(t *testing.T) {
	c, f := newOAuthFixture(t)
	f.config.ClientID = "AnotherNative"
	if _, err := c.AuthConfig(context.Background()); err == nil || !strings.Contains(err.Error(), "clientId") {
		t.Fatal("different Logto client accepted")
	}
}

func TestExchangeRejectsWrongPurposeOrApplication(t *testing.T) {
	for _, tc := range []struct{ name, purpose, app string }{{"wrong-purpose", "console", "NativeA"}, {"wrong-app", "device", "nativea"}, {"legacy-response", "", ""}} {
		t.Run(tc.name, func(t *testing.T) {
			var requests atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path == "/api/v1/auth/config" {
					_ = json.NewEncoder(w).Encode(AuthConfig{Provider: "logto", Issuer: "https://fixture.logto.invalid/oidc", ClientID: "NativeA", Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: "https://fixture.logto.invalid/oidc/device/auth", TokenEndpoint: "https://fixture.logto.invalid/oidc/token"})
					return
				}
				requests.Add(1)
				if r.URL.Path != "/api/v1/auth/exchange" {
					t.Error("invalid exchange started device lifecycle")
				}
				var body map[string]string
				_ = json.NewDecoder(r.Body).Decode(&body)
				if body["appId"] != "NativeA" {
					t.Error("application changed in exchange")
				}
				_ = json.NewEncoder(w).Encode(Session{Purpose: tc.purpose, AppID: tc.app, SessionKey: "test-session", DeviceID: "test-device", ExpiresAt: time.Now().Add(time.Hour), User: User{ID: "test-user"}})
			}))
			defer server.Close()
			c, err := New(Config{BaseURL: server.URL, AppID: "NativeA", DisableTunnel: true})
			if err != nil {
				t.Fatal(err)
			}
			if _, err = c.LoginWithAccessToken(context.Background(), "test-access-token", "test"); err == nil {
				t.Fatal("incorrect exchange response accepted")
			}
			if c.Session().SessionKey != "" || requests.Load() != 1 {
				t.Fatal("bad exchange mutated session or started heartbeat")
			}
		})
	}
}

func TestAppsRequiresSessionAndReadsLogtoDirectory(t *testing.T) {
	control := newTestControl(t)
	c, err := New(Config{BaseURL: control.server.URL, AppID: "NativeTestA", DisableTunnel: true})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	if _, err = c.Apps(context.Background()); err == nil {
		t.Fatal("unauthenticated directory access succeeded")
	}
	session, err := c.LoginWithAccessToken(context.Background(), "fixture-user-token", "test")
	if err != nil {
		t.Fatal(err)
	}
	if session.Purpose != "device" || session.AppID != "NativeTestA" {
		t.Fatal("positive session lost application binding")
	}
	apps, err := c.Apps(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(apps) != 2 || apps[1].ID != "NativeTestB" || apps[1].Type != "Native" || !apps[1].IsDeviceFlow {
		t.Fatal("read-only tenant directory was altered")
	}
}

func TestAppsRejectsOldOrUnsupportedDirectoryResponses(t *testing.T) {
	for _, tc := range []struct {
		name  string
		body  any
		valid bool
	}{
		{"empty-valid", map[string]any{"source": "logto", "apps": []any{}}, true},
		{"old-catalog", map[string]any{"apps": []any{}}, false},
		{"wrong-source", map[string]any{"source": "local", "apps": []any{}}, false},
		{"missing-apps", map[string]any{"source": "logto"}, false},
		{"old-enabled-dto", map[string]any{"source": "logto", "apps": []any{map[string]any{"id": "NativeA", "name": "A", "enabled": true}}}, false},
		{"wrong-type", map[string]any{"source": "logto", "apps": []Application{{ID: "SPA", Type: "SPA", IsDeviceFlow: true}}}, false},
		{"flow-disabled", map[string]any{"source": "logto", "apps": []Application{{ID: "NativeA", Type: "Native"}}}, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.Header.Get("Authorization") != "Bearer test-session" {
					t.Error("directory missing session bearer")
				}
				_ = json.NewEncoder(w).Encode(tc.body)
			}))
			defer server.Close()
			c, err := New(Config{BaseURL: server.URL, AppID: "NativeA"})
			if err != nil {
				t.Fatal(err)
			}
			// Directly supply a test session to isolate this response boundary.
			c.session = Session{Purpose: "device", AppID: "NativeA", SessionKey: "test-session", ExpiresAt: time.Now().Add(time.Hour)}
			apps, err := c.Apps(context.Background())
			if tc.valid {
				if err != nil || apps == nil {
					t.Fatal("valid empty directory rejected")
				}
			} else if err == nil {
				t.Fatal("invalid directory accepted")
			}
		})
	}
}
