package cybergrid

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// AuthConfig contains only public OAuth configuration. No client secret is used.
type AuthConfig struct {
	Coordination                Coordination `json:"coordination,omitempty"`
	Provider                    string       `json:"provider"`
	Issuer                      string       `json:"issuer"`
	ClientID                    string       `json:"clientId"`
	Resource                    string       `json:"resource"`
	Scope                       string       `json:"scope"`
	DeviceAuthorizationEndpoint string       `json:"deviceAuthorizationEndpoint"`
	TokenEndpoint               string       `json:"tokenEndpoint"`
	RevocationEndpoint          string       `json:"revocationEndpoint,omitempty"`
}

// DeviceAuthorization is bound to the client and configuration which created it.
// Only the verification URLs and user code may be displayed to the user.
type DeviceAuthorization struct {
	UserCode                string `json:"userCode"`
	VerificationURI         string `json:"verificationUri"`
	VerificationURIComplete string `json:"verificationUriComplete,omitempty"`
	ExpiresIn               int    `json:"expiresIn"`
	Interval                int    `json:"interval"`
	deviceCode              string
	expiresAt               time.Time
	config                  AuthConfig
	client                  *Client
}

func (a DeviceAuthorization) String() string   { return "DeviceAuthorization{verification required}" }
func (a DeviceAuthorization) GoString() string { return a.String() }

// OAuthTokens are sensitive. Save rotating refresh tokens securely; never log
// tokens or send the ID token in place of a resource access token.
type OAuthTokens struct {
	AccessToken  string    `json:"-"`
	RefreshToken string    `json:"-"`
	ExpiresIn    int       `json:"expiresIn"`
	ExpiresAt    time.Time `json:"expiresAt"`
	TokenType    string    `json:"tokenType"`
	Scope        string    `json:"scope"`
}

func (t OAuthTokens) String() string   { return "OAuthTokens{redacted}" }
func (t OAuthTokens) GoString() string { return t.String() }

// OAuthError exposes only the machine-readable error code, never provider error
// descriptions which may contain credentials or request bodies.
type OAuthError struct {
	Code   string
	Status int
}

func (e *OAuthError) Error() string { return fmt.Sprintf("Logto OAuth %s (%d)", e.Code, e.Status) }

func httpsURL(raw string) (*url.URL, error) {
	u, err := url.Parse(raw)
	if err != nil || u.Scheme != "https" || u.Host == "" || u.User != nil || u.Fragment != "" {
		return nil, errors.New("OAuth URL must be HTTPS without user information or fragment")
	}
	return u, nil
}

func validateAuthConfig(a AuthConfig) error {
	issuer, err := httpsURL(a.Issuer)
	if err != nil {
		return err
	}
	if issuer.RawQuery != "" || a.Provider != "logto" || a.ClientID == "" || a.Resource == "" {
		return errors.New("invalid Logto configuration")
	}
	if _, err := httpsURL(a.Resource); err != nil {
		return errors.New("OAuth resource must be an HTTPS identifier")
	}
	if a.DeviceAuthorizationEndpoint == "" || a.TokenEndpoint == "" {
		return errors.New("OAuth device and token endpoints are required")
	}
	for _, endpoint := range []string{a.DeviceAuthorizationEndpoint, a.TokenEndpoint, a.RevocationEndpoint} {
		if endpoint == "" {
			continue
		}
		u, err := httpsURL(endpoint)
		if err != nil || u.Host != issuer.Host || u.RawQuery != "" {
			return errors.New("OAuth endpoints must be HTTPS on the issuer origin")
		}
	}
	for _, required := range []string{"openid", "offline_access", "connect"} {
		found := false
		for _, scope := range strings.Fields(a.Scope) {
			if scope == required {
				found = true
			}
		}
		if !found {
			return fmt.Errorf("OAuth configuration is missing %s scope", required)
		}
	}
	return nil
}

var errAuthBinding = errors.New("OAuth configuration changed during this client lifecycle; create a new client")

func (c *Client) AuthConfig(ctx context.Context) (AuthConfig, error) {
	var a AuthConfig
	if err := c.request(ctx, "GET", "/auth/config?"+url.Values{"appId": {c.cfg.AppID}}.Encode(), nil, &a, false); err != nil {
		return a, err
	}
	if err := validateAuthConfig(a); err != nil {
		return a, err
	}
	if a.ClientID != c.cfg.AppID {
		return AuthConfig{}, errors.New("Logto auth config clientId does not match Config.AppID")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.authConfig != nil && !sameAuthConfig(*c.authConfig, a) {
		return AuthConfig{}, errAuthBinding
	}
	copy := a
	c.authConfig = &copy
	return a, nil
}

func safeOAuthError(code string) string {
	switch code {
	case "invalid_request", "unauthorized_client", "access_denied", "unsupported_response_type", "invalid_scope", "server_error", "temporarily_unavailable", "invalid_client", "invalid_grant", "unsupported_grant_type", "authorization_pending", "slow_down", "expired_token", "invalid_target":
		return code
	default:
		return "invalid_response"
	}
}

func (c *Client) oauthRequest(ctx context.Context, endpoint string, form url.Values, out any) error {
	req, err := http.NewRequestWithContext(ctx, "POST", endpoint, strings.NewReader(form.Encode()))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Accept", "application/json")
	res, err := c.cfg.HTTPClient.Do(req)
	if err != nil {
		return err
	}
	defer res.Body.Close()
	b, err := io.ReadAll(io.LimitReader(res.Body, (64<<10)+1))
	if err != nil {
		return err
	}
	if len(b) > 64<<10 {
		return errors.New("OAuth response exceeds 64 KiB")
	}
	if res.StatusCode < 200 || res.StatusCode >= 300 {
		var e struct {
			Code string `json:"error"`
		}
		_ = json.Unmarshal(b, &e)
		e.Code = safeOAuthError(e.Code)
		return &OAuthError{Code: e.Code, Status: res.StatusCode}
	}
	if err = json.Unmarshal(b, out); err != nil {
		return errors.New("invalid OAuth JSON response")
	}
	return nil
}

func (c *Client) BeginDeviceAuthorization(ctx context.Context) (DeviceAuthorization, error) {
	a, err := c.AuthConfig(ctx)
	if err != nil {
		return DeviceAuthorization{}, err
	}
	var raw struct {
		DeviceCode              string `json:"device_code"`
		UserCode                string `json:"user_code"`
		VerificationURI         string `json:"verification_uri"`
		VerificationURIComplete string `json:"verification_uri_complete"`
		ExpiresIn               int    `json:"expires_in"`
		Interval                int    `json:"interval"`
	}
	err = c.oauthRequest(ctx, a.DeviceAuthorizationEndpoint, url.Values{"client_id": {a.ClientID}, "scope": {a.Scope}, "resource": {a.Resource}}, &raw)
	if err != nil {
		return DeviceAuthorization{}, err
	}
	if raw.DeviceCode == "" || raw.UserCode == "" || raw.ExpiresIn < 1 || raw.ExpiresIn > 86400 || raw.Interval < 0 || raw.Interval > raw.ExpiresIn {
		return DeviceAuthorization{}, errors.New("invalid device authorization response")
	}
	issuer, _ := httpsURL(a.Issuer)
	verification, validationErr := httpsURL(raw.VerificationURI)
	if validationErr != nil || verification.Host != issuer.Host {
		err = errors.New("verification URL must match pinned issuer HTTPS origin")
	}
	if err != nil {
		return DeviceAuthorization{}, err
	}
	if raw.VerificationURIComplete != "" {
		complete, e := httpsURL(raw.VerificationURIComplete)
		if e != nil || complete.Host != issuer.Host {
			return DeviceAuthorization{}, errors.New("verification URL must match pinned issuer HTTPS origin")
		}
	}
	if raw.Interval < 5 {
		raw.Interval = 5
	}
	return DeviceAuthorization{UserCode: raw.UserCode, VerificationURI: raw.VerificationURI, VerificationURIComplete: raw.VerificationURIComplete, ExpiresIn: raw.ExpiresIn, Interval: raw.Interval, deviceCode: raw.DeviceCode, expiresAt: time.Now().Add(time.Duration(raw.ExpiresIn) * time.Second), config: a, client: c}, nil
}

func (c *Client) tokenRequest(ctx context.Context, a AuthConfig, form url.Values) (OAuthTokens, error) {
	var raw struct {
		AccessToken  string `json:"access_token"`
		RefreshToken string `json:"refresh_token"`
		ExpiresIn    int    `json:"expires_in"`
		TokenType    string `json:"token_type"`
		Scope        string `json:"scope"`
	}
	if err := c.oauthRequest(ctx, a.TokenEndpoint, form, &raw); err != nil {
		return OAuthTokens{}, err
	}
	if raw.AccessToken == "" || raw.ExpiresIn < 1 || !strings.EqualFold(raw.TokenType, "Bearer") {
		return OAuthTokens{}, errors.New("invalid OAuth token response")
	}
	return OAuthTokens{AccessToken: raw.AccessToken, RefreshToken: raw.RefreshToken, ExpiresIn: raw.ExpiresIn, ExpiresAt: time.Now().Add(time.Duration(raw.ExpiresIn) * time.Second), TokenType: raw.TokenType, Scope: raw.Scope}, nil
}

// CompleteDeviceAuthorization polls no faster than the provider interval (at
// least five seconds), obeys slow_down, and stops on denial, expiry or cancellation.
// After approval it exchanges the initial refresh token for a resource access
// token. Logto's initial device-grant access token may be opaque OIDC-only and
// must never be returned as the API token. The latest refresh token is returned.
func (c *Client) CompleteDeviceAuthorization(ctx context.Context, auth DeviceAuthorization) (OAuthTokens, error) {
	c.grantMu.Lock()
	defer c.grantMu.Unlock()
	if c.pendingGrant != nil {
		return OAuthTokens{}, errors.New("pending grant must be saved or revoked before authorizing again")
	}
	if auth.client != c || auth.deviceCode == "" || auth.expiresAt.IsZero() {
		return OAuthTokens{}, errors.New("device authorization belongs to a different client or is invalid")
	}
	ctx, cancel := context.WithDeadline(ctx, auth.expiresAt)
	defer cancel()
	interval := time.Duration(auth.Interval) * time.Second
	if interval < 5*time.Second {
		interval = 5 * time.Second
	}
	for {
		timer := time.NewTimer(interval)
		select {
		case <-ctx.Done():
			timer.Stop()
			return OAuthTokens{}, ctx.Err()
		case <-timer.C:
		}
		// Once a token request is sent, do not drop a minted grant on UI cancellation.
		requestCtx, stopRequest := context.WithTimeout(context.WithoutCancel(ctx), 30*time.Second)
		tokens, err := c.tokenRequest(requestCtx, auth.config, url.Values{"client_id": {auth.config.ClientID}, "grant_type": {"urn:ietf:params:oauth:grant-type:device_code"}, "device_code": {auth.deviceCode}})
		stopRequest()
		if err == nil {
			if tokens.RefreshToken == "" {
				return OAuthTokens{}, errors.New("Logto device authorization did not return an offline refresh token; resource access requires offline_access")
			}
			if saveErr := c.checkpointGrant(ctx, tokens); saveErr != nil {
				return OAuthTokens{}, saveErr
			}
			resourceCtx, stopResource := context.WithTimeout(context.WithoutCancel(ctx), 30*time.Second)
			resourceTokens, refreshErr := c.refreshAccessToken(resourceCtx, tokens.RefreshToken)
			stopResource()
			if refreshErr != nil {
				return OAuthTokens{}, fmt.Errorf("obtain Logto resource access token after device authorization: %w", refreshErr)
			}
			// OAuth permits providers to retain the existing refresh token instead
			// of issuing a replacement. Never discard a replacement when present.
			if resourceTokens.RefreshToken == "" {
				resourceTokens.RefreshToken = tokens.RefreshToken
			}
			if saveErr := c.checkpointGrant(ctx, resourceTokens); saveErr != nil {
				return OAuthTokens{}, saveErr
			}
			return resourceTokens, nil
		}
		var oe *OAuthError
		if !errors.As(err, &oe) || oe.Status != http.StatusBadRequest {
			return OAuthTokens{}, err
		}
		switch oe.Code {
		case "authorization_pending":
		case "slow_down":
			interval += 5 * time.Second
		default:
			return OAuthTokens{}, err
		}
	}
}

// RefreshAccessToken requests a resource JWT. The caller must atomically persist
// a returned refresh token before using it again, and serialize concurrent refreshes.
func (c *Client) RefreshAccessToken(ctx context.Context, refreshToken string) (OAuthTokens, error) {
	c.grantMu.Lock()
	defer c.grantMu.Unlock()
	if c.pendingGrant != nil {
		latestRefresh := c.pendingGrant.RefreshToken
		if c.grantCheckpoint == nil {
			return OAuthTokens{}, errors.New("pending grant must be checkpointed before refreshing")
		}
		if err := c.savePending(ctx); err != nil {
			return OAuthTokens{}, err
		}
		if refreshToken != latestRefresh {
			return OAuthTokens{}, errors.New("latest pending grant checkpointed; reload private credentials before refreshing")
		}
	}
	requestCtx, stop := context.WithTimeout(context.WithoutCancel(ctx), 30*time.Second)
	defer stop()
	t, err := c.refreshAccessToken(requestCtx, refreshToken)
	if err != nil {
		return OAuthTokens{}, err
	}
	if t.RefreshToken == "" {
		t.RefreshToken = refreshToken
	}
	if err = c.checkpointGrant(ctx, t); err != nil {
		return OAuthTokens{}, err
	}
	return t, nil
}
func (c *Client) refreshAccessToken(ctx context.Context, refreshToken string) (OAuthTokens, error) {
	if refreshToken == "" {
		return OAuthTokens{}, errors.New("refresh token is required")
	}
	a, err := c.AuthConfig(ctx)
	if err != nil {
		return OAuthTokens{}, err
	}
	return c.tokenRequest(ctx, a, url.Values{"client_id": {a.ClientID}, "grant_type": {"refresh_token"}, "refresh_token": {refreshToken}, "resource": {a.Resource}})
}

// RevokeRefreshToken revokes the Logto grant if a revocation endpoint is
// advertised. This does not sign out other browser sessions or CyberGrid peers.
func (c *Client) RevokeRefreshToken(ctx context.Context, refreshToken string) error {
	if refreshToken == "" {
		return errors.New("refresh token is required")
	}
	a, err := c.AuthConfig(ctx)
	if err != nil {
		return err
	}
	endpoint := a.RevocationEndpoint
	if endpoint == "" {
		req, err := http.NewRequestWithContext(ctx, "GET", strings.TrimRight(a.Issuer, "/")+"/.well-known/openid-configuration", nil)
		if err != nil {
			return err
		}
		res, err := c.cfg.HTTPClient.Do(req)
		if err != nil {
			return err
		}
		defer res.Body.Close()
		b, err := io.ReadAll(io.LimitReader(res.Body, (64<<10)+1))
		if err != nil {
			return err
		}
		if res.StatusCode != http.StatusOK || len(b) > 64<<10 {
			return errors.New("OIDC discovery failed")
		}
		var discovery struct {
			Issuer             string `json:"issuer"`
			RevocationEndpoint string `json:"revocation_endpoint"`
		}
		if err = json.Unmarshal(b, &discovery); err != nil || discovery.Issuer != a.Issuer {
			return errors.New("OIDC discovery issuer mismatch")
		}
		endpoint = discovery.RevocationEndpoint
		if endpoint == "" {
			return errors.New("Logto does not advertise refresh token revocation; local logout is still available")
		}
		a.RevocationEndpoint = endpoint
		if err = validateAuthConfig(a); err != nil {
			return err
		}
	}
	req, err := http.NewRequestWithContext(ctx, "POST", endpoint, strings.NewReader(url.Values{"client_id": {a.ClientID}, "token": {refreshToken}, "token_type_hint": {"refresh_token"}}.Encode()))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	res, err := c.cfg.HTTPClient.Do(req)
	if err != nil {
		return err
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		return &OAuthError{Code: "revocation_failed", Status: res.StatusCode}
	}
	return nil
}

func sameAuthConfig(a, b AuthConfig) bool {
	a.Coordination = Coordination{}
	b.Coordination = Coordination{}
	return a == b
}

// CachedAuthConfig returns already verified public binding data without I/O.
func (c *Client) CachedAuthConfig() (AuthConfig, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.authConfig == nil {
		return AuthConfig{}, false
	}
	return *c.authConfig, true
}
