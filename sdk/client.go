package cybergrid

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode/utf8"
)

type Client struct {
	grantMu               sync.Mutex
	grantCheckpoint       GrantCheckpoint
	pendingGrant          *OAuthTokens
	registrationRequestID string
	cfg                   Config
	publishMu             sync.Mutex
	recoverMu             sync.Mutex
	memory                bool
	registrationKey       string
	registrationRevision  int64
	publishRevision       int64
	generation            uint64
	ready                 bool
	deviceName            string
	initialUser           string
	lifetime              context.Context
	lifetimeCancel        context.CancelFunc
	authProvider          AuthProvider
	mappings              map[*Mapping]struct{}
	authConfig            *AuthConfig
	mu                    sync.Mutex
	session               Session
	cert                  tls.Certificate
	fingerprint           string
	ports                 map[string]Port
	ctx                   context.Context
	cancel                context.CancelFunc
	peers                 map[string]*Peer
	pending               map[string]*rtcAttempt
	cursor                int64
	origin                net.Listener
	server                *http.Server
	tunnel                *Tunnel
	tunnelURL             string
	tunnelErr             error
	lastHeartbeat         time.Time
	closing               bool
	wg                    sync.WaitGroup
	// Only used by in-package network tests; never exported or enabled by configuration.
	testTunnelURL string
}

func New(cfg Config) (*Client, error) {
	u, err := url.Parse(cfg.BaseURL)
	if err != nil || u.Host == "" || u.User != nil || u.RawQuery != "" || u.Fragment != "" {
		return nil, errors.New("invalid base URL")
	}
	if u.Scheme != "https" && !(u.Scheme == "http" && (u.Hostname() == "localhost" || net.ParseIP(u.Hostname()) != nil && net.ParseIP(u.Hostname()).IsLoopback())) {
		return nil, errors.New("base URL must use HTTPS (HTTP is allowed only on loopback)")
	}
	if err := validateAppID(cfg.AppID); err != nil {
		return nil, err
	}
	cfg.BaseURL = strings.TrimRight(cfg.BaseURL, "/")
	if cfg.HTTPClient == nil {
		cfg.HTTPClient = &http.Client{Timeout: 30 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	} else {
		clone := *cfg.HTTPClient
		clone.CheckRedirect = func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
		cfg.HTTPClient = &clone
	}
	if cfg.DirectTimeout == 0 {
		cfg.DirectTimeout = 12 * time.Second
	}
	if cfg.FallbackTimeout == 0 {
		cfg.FallbackTimeout = 35 * time.Second
	}
	if cfg.FallbackTimeout < 0 {
		return nil, errors.New("fallback timeout must be positive")
	}
	if cfg.DirectTimeout < 0 {
		return nil, errors.New("direct timeout must be positive")
	}
	if cfg.PollInterval == 0 {
		cfg.PollInterval = time.Second
	}
	if cfg.PollInterval < 0 {
		return nil, errors.New("poll interval must be positive")
	}
	if cfg.STUNServers == nil {
		cfg.STUNServers = []string{"stun:stun.l.google.com:19302", "stun:stun.cloudflare.com:3478"}
	}
	for _, s := range cfg.STUNServers {
		if !strings.HasPrefix(s, "stun:") {
			return nil, errors.New("only STUN servers are permitted; TURN is disabled")
		}
	}
	return &Client{cfg: cfg, authProvider: cfg.AuthProvider, mappings: map[*Mapping]struct{}{}, ports: map[string]Port{}, peers: map[string]*Peer{}, pending: map[string]*rtcAttempt{}}, nil
}

func (c *Client) request(ctx context.Context, method, path string, body, out any, auth bool) error {
	var r io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return err
		}
		r = bytes.NewReader(b)
	}
	req, err := http.NewRequestWithContext(ctx, method, c.cfg.BaseURL+"/api/v1"+path, r)
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	var usedKey, usedEpoch string
	if auth {
		c.mu.Lock()
		key := c.session.SessionKey
		expiry := c.session.ExpiresAt
		usedKey = key
		usedEpoch = c.session.CoordinatorEpoch
		c.mu.Unlock()
		if key == "" || method != "DELETE" && !time.Now().Before(expiry) {
			return &APIError{Status: 401, Code: "SESSION_EXPIRED", Message: "login required"}
		}
		req.Header.Set("Authorization", "Bearer "+key)
		if usedEpoch != "" {
			req.Header.Set("X-CyberGrid-Epoch", usedEpoch)
		}
	}
	res, err := c.cfg.HTTPClient.Do(req)
	if err != nil {
		return err
	}
	defer res.Body.Close()
	if auth && method != "DELETE" {
		c.mu.Lock()
		stale := c.session.SessionKey != usedKey || c.session.CoordinatorEpoch != usedEpoch
		c.mu.Unlock()
		if stale {
			return &APIError{Status: 409, Code: "STALE_GENERATION", Message: "old response ignored"}
		}
		if epoch := res.Header.Get("X-CyberGrid-Epoch"); usedEpoch != "" && epoch != "" && epoch != usedEpoch {
			return &APIError{Status: 409, Code: "COORDINATOR_RESET", Message: "coordinator restarted"}
		}
	}
	data, err := io.ReadAll(io.LimitReader(res.Body, (2<<20)+1))
	if auth && method != "DELETE" {
		c.mu.Lock()
		stale := c.session.SessionKey != usedKey || c.session.CoordinatorEpoch != usedEpoch
		c.mu.Unlock()
		if stale {
			return &APIError{Status: 409, Code: "STALE_GENERATION", Message: "old response ignored"}
		}
	}
	if err != nil {
		return err
	}
	if len(data) > 2<<20 {
		return errors.New("API response exceeds 2 MiB")
	}
	if res.StatusCode < 200 || res.StatusCode >= 300 {
		var envelope struct {
			Error APIError `json:"error"`
		}
		_ = json.Unmarshal(data, &envelope)
		envelope.Error.Status = res.StatusCode
		if seconds, e := strconv.Atoi(res.Header.Get("Retry-After")); e == nil && seconds > 0 {
			if seconds > 60 {
				seconds = 60
			}
			envelope.Error.RetryAfter = time.Duration(seconds) * time.Second
		}
		if envelope.Error.Code == "" {
			envelope.Error.Code = "HTTP_ERROR"
			envelope.Error.Message = http.StatusText(res.StatusCode)
		}
		return &envelope.Error
	}
	if out != nil {
		if err = json.Unmarshal(data, out); err != nil {
			return fmt.Errorf("invalid API response: %w", err)
		}
	}
	return nil
}

// Register no longer sends account credentials. Registration is hosted by Logto.
// Deprecated: use BeginDeviceAuthorization and CompleteDeviceAuthorization.
func (c *Client) Register(ctx context.Context, email, secret string) (User, bool, error) {
	return User{}, false, errors.New("account registration is hosted by Logto; use device authorization")
}

// Login no longer accepts account credentials.
// Deprecated: use LoginWithAccessToken after Logto device authorization.
func (c *Client) Login(ctx context.Context, email, secret, name string) (Session, error) {
	return Session{}, errors.New("account credential login is unsupported; use Logto device authorization and LoginWithAccessToken")
}
func validateAppID(id string) error {
	if !utf8.ValidString(id) || utf8.RuneCountInString(id) < 1 || utf8.RuneCountInString(id) > 21 {
		return errors.New("Logto Native application ID must contain 1..21 characters; set Config.AppID explicitly")
	}
	return nil
}

// Apps lists the tenant's read-only standard Logto Native directory using
// this client's device session. It is not a per-user authorization list.
func (c *Client) Apps(ctx context.Context) ([]Application, error) {
	var out struct {
		Source string        `json:"source"`
		Apps   []Application `json:"apps"`
	}
	if err := c.request(ctx, "GET", "/apps", nil, &out, true); err != nil {
		return nil, err
	}
	if out.Source != "logto" || out.Apps == nil {
		return nil, errors.New("invalid Logto application directory response")
	}
	seen := make(map[string]bool)
	for _, app := range out.Apps {
		if validateAppID(app.ID) != nil || app.Type != "Native" || app.IsDeviceFlow || seen[app.ID] {
			return nil, errors.New("application directory contains an invalid or duplicate Native browser sign-in entry")
		}
		seen[app.ID] = true
	}
	return out.Apps, nil
}
func (c *Client) LoginWithAccessToken(ctx context.Context, accessToken, name string) (Session, error) {
	a, configErr := c.AuthConfig(ctx)
	if configErr != nil {
		return Session{}, configErr
	}
	if a.Coordination.Protocol == "memory-v1" {
		return c.loginMemory(ctx, accessToken, name, a)
	}
	if c.authProvider != nil {
		return Session{}, errors.New("automatic recovery requires memory-v1 coordinator")
	}
	if strings.TrimSpace(accessToken) == "" {
		return Session{}, errors.New("Logto resource access token is required")
	}
	c.mu.Lock()
	busy := c.session.SessionKey != "" || c.closing
	c.mu.Unlock()
	if busy {
		return Session{}, errors.New("client already logged in or closed; create a new client")
	}
	var s Session
	err := c.request(ctx, "POST", "/auth/exchange", map[string]string{"accessToken": accessToken, "appId": c.cfg.AppID, "deviceName": name}, &s, false)
	if err != nil {
		return s, err
	}
	if s.Purpose != "device" || s.AppID != c.cfg.AppID || s.DeviceID == "" || s.SessionKey == "" || !s.ExpiresAt.After(time.Now()) {
		return Session{}, errors.New("invalid device session response: purpose/appId must match this Logto application")
	}
	cert, fp, err := newCertificate(s.DeviceID)
	if err != nil {
		return Session{}, err
	}
	c.mu.Lock()
	c.session = s
	c.cert = cert
	c.fingerprint = fp
	c.ctx, c.cancel = context.WithCancel(context.Background())
	c.lastHeartbeat = time.Now()
	c.mu.Unlock()
	if err = c.startOrigin(); err != nil {
		c.Close()
		return Session{}, err
	}
	if err = c.heartbeat(ctx); err != nil {
		c.Close()
		return Session{}, err
	}
	c.wg.Add(1)
	go c.run()
	if !c.cfg.DisableTunnel {
		c.wg.Add(1)
		go c.startTunnel()
	}
	return s, nil
}
func (c *Client) Session() Session { c.mu.Lock(); defer c.mu.Unlock(); return c.session }
func (c *Client) Devices(ctx context.Context) ([]Device, error) {
	var out struct {
		Devices []Device `json:"devices"`
	}
	err := c.request(ctx, "GET", "/devices", nil, &out, true)
	return out.Devices, err
}
func (c *Client) device(ctx context.Context, id string) (Device, error) {
	ds, err := c.Devices(ctx)
	if err != nil {
		return Device{}, err
	}
	for _, d := range ds {
		if d.ID == id && d.Online && d.CertificateFingerprint != nil {
			return d, nil
		}
	}
	return Device{}, fmt.Errorf("device %s is not online in this account and app", id)
}
func (c *Client) Expose(ctx context.Context, p Port) error {
	c.publishMu.Lock()
	defer c.publishMu.Unlock()
	if p.ID == "" || len(p.ID) > 64 || p.Port < 1 || p.Port > 65535 || strings.TrimSpace(p.Type) == "" || len(p.Type) > 128 || p.Transport != "tcp" {
		return errors.New("invalid TCP port; id, type, transport=tcp and port 1..65535 are required")
	}
	c.mu.Lock()
	old, exists := c.ports[p.ID]
	if !exists && len(c.ports) >= 32 {
		c.mu.Unlock()
		return errors.New("maximum 32 ports")
	}
	c.ports[p.ID] = p
	c.mu.Unlock()
	if err := c.heartbeatLocked(ctx); err != nil {
		c.mu.Lock()
		if exists {
			c.ports[p.ID] = old
		} else {
			delete(c.ports, p.ID)
		}
		c.mu.Unlock()
		return err
	}
	return nil
}
func (c *Client) Unexpose(ctx context.Context, id string) error {
	c.publishMu.Lock()
	defer c.publishMu.Unlock()
	c.mu.Lock()
	p, ok := c.ports[id]
	delete(c.ports, id)
	c.mu.Unlock()
	if !ok {
		return errors.New("port is not exposed")
	}
	if err := c.heartbeatLocked(ctx); err != nil {
		c.mu.Lock()
		c.ports[id] = p
		c.mu.Unlock()
		return err
	}
	return nil
}
func (c *Client) heartbeat(ctx context.Context) error {
	c.publishMu.Lock()
	defer c.publishMu.Unlock()
	return c.heartbeatLocked(ctx)
}
func (c *Client) heartbeatLocked(ctx context.Context) error {
	c.mu.Lock()
	ps := make([]Port, 0, len(c.ports))
	for _, p := range c.ports {
		ps = append(ps, p)
	}
	fp := c.fingerprint
	tu := c.tunnelURL
	c.mu.Unlock()
	sort.Slice(ps, func(i, j int) bool { return ps[i].ID < ps[j].ID })
	body := map[string]any{"ports": ps, "certificateFingerprint": fp}
	c.mu.Lock()
	if c.memory {
		if c.publishRevision >= maxSafeRevision {
			c.mu.Unlock()
			return errors.New("publication revision exhausted; explicit client reauthorization required")
		}
		c.publishRevision++
		body["publishRevision"] = c.publishRevision
		body["incarnation"] = c.session.Incarnation
	}
	c.mu.Unlock()
	if tu != "" {
		body["tunnelUrl"] = tu
	}
	if err := c.request(ctx, "PUT", "/devices/self", body, nil, true); err != nil {
		return err
	}
	c.mu.Lock()
	c.lastHeartbeat = time.Now()
	c.mu.Unlock()
	return nil
}
func (c *Client) report(err error) {
	if err != nil && c.cfg.OnError != nil {
		c.cfg.OnError(err)
	}
}
func (c *Client) run() {
	defer c.wg.Done()
	c.mu.Lock()
	memory := c.memory
	c.mu.Unlock()
	if memory {
		c.runMemory()
		return
	}
	poll := time.NewTicker(c.cfg.PollInterval)
	beat := time.NewTicker(20 * time.Second)
	defer poll.Stop()
	defer beat.Stop()
	for {
		select {
		case <-c.ctx.Done():
			return
		case <-beat.C:
			if err := c.heartbeat(c.ctx); err != nil {
				c.report(fmt.Errorf("heartbeat: %w", err))
				if c.fatal(err) {
					c.shutdown()
					return
				}
			}
		case <-poll.C:
			c.mu.Lock()
			expired := time.Now().After(c.session.ExpiresAt) || time.Since(c.lastHeartbeat) > 65*time.Second
			cursor := c.cursor
			c.mu.Unlock()
			if expired {
				c.report(errors.New("session expired or heartbeat lease lost"))
				c.shutdown()
				return
			}
			var out struct {
				Signals []Signal `json:"signals"`
				Cursor  int64    `json:"cursor"`
			}
			err := c.request(c.ctx, "GET", "/signals?after="+strconv.FormatInt(cursor, 10), nil, &out, true)
			if err != nil {
				if c.ctx.Err() != nil {
					return
				}
				c.report(fmt.Errorf("signaling: %w", err))
				if c.fatal(err) {
					c.shutdown()
					return
				}
				continue
			}
			if out.Cursor < cursor {
				c.report(errors.New("signaling cursor moved backwards"))
				continue
			}
			for _, s := range out.Signals {
				if err := c.handleSignal(s); err != nil {
					c.report(err)
				}
			}
			c.mu.Lock()
			c.cursor = out.Cursor
			c.mu.Unlock()
		}
	}
}
func (c *Client) fatal(err error) bool {
	var a *APIError
	return errors.As(err, &a) && (a.Status == 401 || a.Status == 403)
}
func (c *Client) sendSignal(ctx context.Context, id, connection, kind string, p any) error {
	return c.sendMemorySignal(ctx, id, connection, kind, p)
}
func (c *Client) shutdown() {
	c.mu.Lock()
	if c.cancel != nil {
		c.cancel()
	}
	peers := make([]*Peer, 0, len(c.peers))
	for _, p := range c.peers {
		peers = append(peers, p)
	}
	attempts := make([]*rtcAttempt, 0, len(c.pending))
	for _, a := range c.pending {
		attempts = append(attempts, a)
	}
	srv := c.server
	tu := c.tunnel
	c.mu.Unlock()
	for _, a := range attempts {
		a.close()
	}
	for _, p := range peers {
		p.Close()
	}
	if srv != nil {
		_ = srv.Close()
	}
	if tu != nil && tu.Close != nil {
		_ = tu.Close()
	}
}
func (c *Client) Close() error {
	c.mu.Lock()
	if c.closing {
		c.mu.Unlock()
		return nil
	}
	c.closing = true
	if c.lifetimeCancel != nil {
		c.lifetimeCancel()
	}
	mappings := make([]*Mapping, 0, len(c.mappings))
	for m := range c.mappings {
		mappings = append(mappings, m)
	}
	logged := c.session.SessionKey != ""
	c.mu.Unlock()
	c.shutdown()
	for _, m := range mappings {
		m.Close()
	}
	c.wg.Wait()
	var err error
	if logged {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		err = c.request(ctx, "DELETE", "/session", nil, nil, true)
	}
	c.mu.Lock()
	c.session.SessionKey = ""
	c.mu.Unlock()
	c.coordinationEvent("closed", "")
	return err
}

func (c *Client) event(phase, transport, peer string) {
	if c.cfg.OnTransportEvent != nil {
		c.cfg.OnTransportEvent(TransportEvent{Phase: phase, Transport: transport, PeerID: peer})
	}
}
