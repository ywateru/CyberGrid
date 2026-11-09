package cybergrid

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/xtaci/smux"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

type testControl struct {
	mu       sync.Mutex
	server   *httptest.Server
	sessions map[string]Session
	devices  map[string]Device
	signals  []struct {
		Signal
		To string
	}
	sequence int64
}

func newTestControl(t *testing.T) *testControl {
	s := &testControl{sessions: map[string]Session{}, devices: map[string]Device{}}
	s.server = httptest.NewServer(http.HandlerFunc(s.serve))
	t.Cleanup(s.server.Close)
	return s
}
func (s *testControl) serve(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	w.Header().Set("Content-Type", "application/json")
	reply := func(v any) { _ = json.NewEncoder(w).Encode(v) }
	path := strings.TrimPrefix(r.URL.Path, "/api/v1")
	if path == "/auth/config" {
		reply(AuthConfig{Provider: "logto", Issuer: "https://fixture.logto.invalid/oidc", ClientID: r.URL.Query().Get("appId"), Resource: "https://cybergrid.aixgg.com/api", Scope: "openid offline_access connect", DeviceAuthorizationEndpoint: "https://fixture.logto.invalid/oidc/device/auth", TokenEndpoint: "https://fixture.logto.invalid/oidc/token"})
		return
	}
	if path == "/auth/exchange" {
		var p map[string]string
		_ = json.NewDecoder(r.Body).Decode(&p)
		id := randomID()
		sess := Session{Purpose: "device", AppID: p["appId"], DeviceID: id, SessionKey: randomID(), ExpiresAt: time.Now().Add(time.Hour), User: User{ID: p["accessToken"], Email: p["accessToken"]}}
		s.sessions[sess.SessionKey] = sess
		s.devices[id] = Device{ID: id, Name: p["deviceName"], Online: true, LastSeen: time.Now(), Ports: []Port{}}
		reply(sess)
		return
	}
	key := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
	sess, ok := s.sessions[key]
	if !ok {
		w.WriteHeader(401)
		reply(map[string]any{"error": map[string]string{"code": "UNAUTHORIZED", "message": "login required"}})
		return
	}
	switch {
	case path == "/apps" && r.Method == "GET":
		reply(map[string]any{"source": "logto", "apps": []Application{{ID: "NativeTestA", Name: "Fixture A", Type: "Native", IsDeviceFlow: false}, {ID: "NativeTestB", Name: "Fixture B", Type: "Native", IsDeviceFlow: false}}})
	case path == "/session" && r.Method == "DELETE":
		delete(s.sessions, key)
		delete(s.devices, sess.DeviceID)
		reply(map[string]bool{"ok": true})
	case path == "/devices/self":
		var p struct {
			Ports       []Port  `json:"ports"`
			TunnelURL   *string `json:"tunnelUrl"`
			Fingerprint string  `json:"certificateFingerprint"`
		}
		_ = json.NewDecoder(r.Body).Decode(&p)
		d := s.devices[sess.DeviceID]
		d.Ports = p.Ports
		d.TunnelURL = p.TunnelURL
		d.CertificateFingerprint = &p.Fingerprint
		d.LastSeen = time.Now()
		s.devices[d.ID] = d
		reply(map[string]Device{"device": d})
	case path == "/devices":
		ds := make([]Device, 0)
		for _, d := range s.devices {
			ds = append(ds, d)
		}
		reply(map[string]any{"devices": ds})
	case path == "/signals" && r.Method == "POST":
		var p struct {
			To         string `json:"toDeviceId"`
			Connection string `json:"connectionId"`
			Kind       string `json:"kind"`
			Payload    string `json:"payload"`
		}
		_ = json.NewDecoder(r.Body).Decode(&p)
		s.sequence++
		v := struct {
			Signal
			To string
		}{Signal: Signal{ID: s.sequence, FromDeviceID: sess.DeviceID, ConnectionID: p.Connection, Kind: p.Kind, Payload: p.Payload}, To: p.To}
		s.signals = append(s.signals, v)
		reply(map[string]int64{"id": s.sequence})
	case path == "/signals":
		var after int64
		_, _ = fmt.Sscan(r.URL.Query().Get("after"), &after)
		signals := []Signal{}
		cursor := after
		for _, v := range s.signals {
			if v.To == sess.DeviceID && v.ID > after {
				signals = append(signals, v.Signal)
				cursor = v.ID
			}
		}
		reply(map[string]any{"signals": signals, "cursor": cursor})
	default:
		w.WriteHeader(404)
		reply(map[string]string{"error": "not found"})
	}
}
func testClients(t *testing.T, tunnel bool) (*Client, *Client, context.Context) {
	t.Helper()
	s := newTestControl(t)
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	t.Cleanup(cancel)
	cfg := Config{OnError: func(err error) { t.Logf("SDK: %v", err) }, BaseURL: s.server.URL, AppID: "NativeTestA", STUNServers: []string{}, DisableTunnel: true, PollInterval: 20 * time.Millisecond, DirectTimeout: 5 * time.Second}
	if tunnel {
		cfg.InterfaceName = "cybergrid-nonexistent"
		cfg.DirectTimeout = 150 * time.Millisecond
	}
	host, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	dialer, err := New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = dialer.Close(); _ = host.Close() })
	for i, c := range []*Client{host, dialer} {
		if _, err = c.LoginWithAccessToken(ctx, "test-resource-token", fmt.Sprint("test-", i)); err != nil {
			t.Fatal(err)
		}
	}
	if tunnel {
		host.mu.Lock()
		host.tunnelURL = "https://test-sdk.trycloudflare.com"
		origin := "http://" + host.origin.Addr().String()
		host.mu.Unlock()
		dialer.mu.Lock()
		dialer.testTunnelURL = origin
		dialer.mu.Unlock()
		if err = host.heartbeat(ctx); err != nil {
			t.Fatal(err)
		}
	}
	return host, dialer, ctx
}
func echoListener(t *testing.T, halfClose bool) net.Listener {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { ln.Close() })
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go func() {
				defer conn.Close()
				if halfClose {
					b, _ := io.ReadAll(conn)
					_, _ = conn.Write(append([]byte("ack:"), b...))
				} else {
					_, _ = io.Copy(conn, conn)
				}
			}()
		}
	}()
	return ln
}
func TestMappingRealTransports(t *testing.T) {
	for _, mode := range []string{"webrtc-direct", "cloudflare-tunnel"} {
		t.Run(mode, func(t *testing.T) {
			host, dialer, ctx := testClients(t, mode == "cloudflare-tunnel")
			ln := echoListener(t, false)
			if err := host.Expose(ctx, Port{ID: "echo", Port: ln.Addr().(*net.TCPAddr).Port, Type: "echo", Transport: "tcp"}); err != nil {
				t.Fatal(err)
			}
			m, err := dialer.Map(ctx, host.Session().DeviceID, "echo", "127.0.0.1:0")
			if err != nil {
				t.Fatal(err)
			}
			defer m.Close()
			if m.Transport != mode {
				t.Fatalf("transport %q", m.Transport)
			}
			conn, err := net.Dial("tcp", m.Addr.String())
			if err != nil {
				t.Fatal(err)
			}
			defer conn.Close()
			_ = conn.SetDeadline(time.Now().Add(5 * time.Second))
			want := bytes.Repeat([]byte("real peer stream\x00"), 20000)
			done := make(chan error, 1)
			go func() { _, err := conn.Write(want); done <- err }()
			actual := make([]byte, len(want))
			if _, err = io.ReadFull(conn, actual); err != nil {
				t.Fatal(err)
			}
			if err = <-done; err != nil {
				t.Fatal(err)
			}
			if !bytes.Equal(actual, want) {
				t.Fatal("payload mismatch")
			}
			if err = m.Peer.Close(); err != nil {
				t.Fatal(err)
			}
			deadline := time.Now().Add(time.Second)
			for {
				probe, err := net.DialTimeout("tcp", m.Addr.String(), 50*time.Millisecond)
				if err != nil {
					break
				}
				probe.Close()
				if time.Now().After(deadline) {
					t.Fatal("mapping listener remained open after peer disconnect")
				}
				time.Sleep(5 * time.Millisecond)
			}
		})
	}
}
func TestTCPHalfCloseAndDeadline(t *testing.T) {
	host, dialer, ctx := testClients(t, true)
	ln := echoListener(t, true)
	if err := host.Expose(ctx, Port{ID: "half", Port: ln.Addr().(*net.TCPAddr).Port, Type: "half", Transport: "tcp"}); err != nil {
		t.Fatal(err)
	}
	m, err := dialer.Map(ctx, host.Session().DeviceID, "half", "")
	if err != nil {
		t.Fatal(err)
	}
	defer m.Close()
	conn, err := net.Dial("tcp", m.Addr.String())
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(3 * time.Second))
	_, _ = conn.Write([]byte("request"))
	if err = conn.(*net.TCPConn).CloseWrite(); err != nil {
		t.Fatal(err)
	}
	b, err := io.ReadAll(conn)
	if err != nil {
		t.Fatal(err)
	}
	if string(b) != "ack:request" {
		t.Fatalf("response %q", b)
	}
	direct, err := m.Peer.Dial(ctx, "half")
	if err != nil {
		t.Fatal(err)
	}
	defer direct.Close()
	_ = direct.SetReadDeadline(time.Now().Add(30 * time.Millisecond))
	var one [1]byte
	if _, err = direct.Read(one[:]); err == nil {
		t.Fatal("read deadline was ignored")
	}
}
func TestTunnelWrongFingerprintRejected(t *testing.T) {
	host, dialer, ctx := testClients(t, true)
	d, err := dialer.device(ctx, host.Session().DeviceID)
	if err != nil {
		t.Fatal(err)
	}
	bad := strings.Repeat("0", 64)
	d.CertificateFingerprint = &bad
	if p, err := dialer.dialTunnel(ctx, d); err == nil {
		p.Close()
		t.Fatal("accepted wrong fingerprint")
	}
}
func TestRevocationClosesMapping(t *testing.T) {
	host, dialer, ctx := testClients(t, true)
	ln := echoListener(t, false)
	if err := host.Expose(ctx, Port{ID: "echo", Port: ln.Addr().(*net.TCPAddr).Port, Type: "echo", Transport: "tcp"}); err != nil {
		t.Fatal(err)
	}
	m, err := dialer.Map(ctx, host.Session().DeviceID, "echo", "")
	if err != nil {
		t.Fatal(err)
	}
	defer m.Close()
	if err = dialer.request(ctx, "DELETE", "/session", nil, nil, true); err != nil {
		t.Fatal(err)
	}
	select {
	case <-dialer.ctx.Done():
	case <-time.After(time.Second):
		t.Fatal("revocation did not cancel client")
	}
	deadline := time.Now().Add(time.Second)
	for {
		conn, err := net.Dial("tcp", m.Addr.String())
		if err != nil {
			break
		}
		conn.Close()
		if time.Now().After(deadline) {
			t.Fatal("listener survived revocation")
		}
		time.Sleep(5 * time.Millisecond)
	}
}
func TestValidation(t *testing.T) {
	for _, v := range []string{"http://x.trycloudflare.com", "https://x.trycloudflare.com.evil.test", "https://a.b.trycloudflare.com", "https://user@x.trycloudflare.com", "https://x.trycloudflare.com:443", "https://x.trycloudflare.com/path", "https://x.trycloudflare.com/?a=1"} {
		if ValidateTunnelURL(v) == nil {
			t.Fatalf("accepted %s", v)
		}
	}
	if err := ValidateTunnelURL("https://quiet-test.trycloudflare.com"); err != nil {
		t.Fatal(err)
	}
	if _, err := New(Config{BaseURL: "http://example.test", AppID: "a"}); err == nil {
		t.Fatal("accepted nonlocal cleartext central")
	}
	if _, err := New(Config{BaseURL: "https://example.test", AppID: "a", STUNServers: []string{"turn:x"}}); err == nil {
		t.Fatal("accepted TURN")
	}
}
func TestProviderExitClearsPublishedURL(t *testing.T) {
	if testing.Short() {
		t.Skip("process test")
	}
	host, _, ctx := testClients(t, false)
	dir := t.TempDir()
	binary := filepath.Join(dir, "provider")
	if err := os.WriteFile(binary, []byte("#!/bin/sh\necho https://test-exit.trycloudflare.com >&2\necho 'Registered tunnel connection connIndex=0 protocol=http2' >&2\nsleep 0.2\n"), 0700); err != nil {
		t.Fatal(err)
	}
	host.cfg.TunnelProvider = Cloudflared{Binary: binary}
	host.wg.Add(1)
	go host.startTunnel()
	deadline := time.Now().Add(2 * time.Second)
	seen := false
	for {
		host.mu.Lock()
		value := host.tunnelURL
		exitErr := host.tunnelErr
		host.mu.Unlock()
		if value != "" {
			seen = true
		}
		if seen && exitErr != nil && value == "" {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("provider URL was not published then revoked")
		}
		select {
		case <-ctx.Done():
			t.Fatal(ctx.Err())
		case <-time.After(10 * time.Millisecond):
		}
	}
}

func TestAllocatedTunnelURLRequiresCompleteAnnouncement(t *testing.T) {
	valid := []string{
		"https://quiet-test.trycloudflare.com",
		"  https://quiet-test.trycloudflare.com  ",
		"2026-11-07T00:00:00Z INF | https://quiet-test.trycloudflare.com |",
	}
	for _, line := range valid {
		if value, ok := allocatedTunnelURL(line); !ok || value != "https://quiet-test.trycloudflare.com" {
			t.Fatalf("did not accept complete allocation announcement %q", line)
		}
	}
	invalid := []string{
		`failed to request quick Tunnel: Post "https://api.trycloudflare.com/tunnel": lookup failed`,
		"https://api.trycloudflare.com",
		"https://quiet-test.trycloudflare.com/tunnel",
		"https://quiet-test.trycloudflare.com?error=true",
		"https://quiet-test.trycloudflare.com.evil.test",
		"ERR unable to connect to https://quiet-test.trycloudflare.com",
	}
	for _, line := range invalid {
		if value, ok := allocatedTunnelURL(line); ok {
			t.Fatalf("accepted an error or partial URL %q as %q", line, value)
		}
	}
}

type providerStartResult struct {
	tunnel *Tunnel
	err    error
}

// Gate the fake provider after its first announcement. Tests measure readiness
// after the shell has started, rather than spending a 1s/2s semantic assertion
// budget on exec/scheduler latency under the race detector.
func gatedProviderFixture(t *testing.T, announcement, afterRelease string) (string, string, string) {
	t.Helper()
	dir := t.TempDir()
	announced := filepath.Join(dir, "announced")
	release := filepath.Join(dir, "release")
	quote := func(path string) string { return "'" + strings.ReplaceAll(path, "'", "'\\''") + "'" }
	script := "#!/bin/sh\nset -eu\n" + announcement + "\n: > " + quote(announced) +
		"\nwhile [ ! -f " + quote(release) + " ]; do sleep 0.01; done\n" + afterRelease + "\n"
	binary := filepath.Join(dir, "provider")
	if err := os.WriteFile(binary, []byte(script), 0700); err != nil {
		t.Fatal(err)
	}
	return binary, announced, release
}

func startProviderForTest(ctx context.Context, binary string) <-chan providerStartResult {
	result := make(chan providerStartResult, 1)
	go func() {
		tunnel, err := (Cloudflared{Binary: binary}).Start(ctx, "http://127.0.0.1:1")
		result <- providerStartResult{tunnel, err}
	}()
	return result
}

func waitProviderAnnouncement(t *testing.T, ctx context.Context, announced string, result <-chan providerStartResult) {
	t.Helper()
	ticker := time.NewTicker(10 * time.Millisecond)
	defer ticker.Stop()
	for {
		if _, err := os.Stat(announced); err == nil {
			return
		} else if !os.IsNotExist(err) {
			t.Fatal(err)
		}
		select {
		case <-ticker.C:
		case got := <-result:
			if got.tunnel != nil {
				_ = got.tunnel.Close()
			}
			t.Fatalf("provider returned before fixture announcement: %v", got.err)
		case <-ctx.Done():
			t.Fatalf("provider fixture did not start before watchdog: %v", ctx.Err())
		}
	}
}

func assertProviderAwaitingReady(t *testing.T, ctx context.Context, result <-chan providerStartResult) {
	t.Helper()
	// The fixture is already running and blocked behind the release file. An
	// allocated URL by itself must not make Start return during this window.
	timer := time.NewTimer(100 * time.Millisecond)
	defer timer.Stop()
	select {
	case got := <-result:
		if got.tunnel != nil {
			_ = got.tunnel.Close()
		}
		t.Fatalf("provider returned without both URL and registered edge: %v", got.err)
	case <-timer.C:
	case <-ctx.Done():
		t.Fatalf("provider watchdog expired before readiness assertion: %v", ctx.Err())
	}
}

func awaitProviderResult(t *testing.T, ctx context.Context, result <-chan providerStartResult) providerStartResult {
	t.Helper()
	select {
	case got := <-result:
		return got
	case <-ctx.Done():
		t.Fatalf("provider did not return before watchdog: %v", ctx.Err())
		return providerStartResult{}
	}
}

func TestProviderDoesNotReturnURLWithoutRegisteredEdge(t *testing.T) {
	cases := []struct{ name, announcement string }{
		{"url-only", "echo https://unready.trycloudflare.com >&2"},
		{"registered-with-invalid-url", "echo 'failed request: Post https://api.trycloudflare.com/tunnel: DNS unavailable' >&2\necho 'Registered tunnel connection' >&2"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			binary, announced, release := gatedProviderFixture(t, tc.announcement, "exit 1")
			ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
			defer cancel()
			result := startProviderForTest(ctx, binary)
			waitProviderAnnouncement(t, ctx, announced, result)
			assertProviderAwaitingReady(t, ctx, result)
			if err := os.WriteFile(release, nil, 0600); err != nil {
				t.Fatal(err)
			}
			got := awaitProviderResult(t, ctx, result)
			if got.tunnel != nil {
				defer got.tunnel.Close()
			}
			if got.err == nil || got.tunnel != nil || !strings.Contains(got.err.Error(), "before ready") {
				t.Fatalf("provider advertised an unusable tunnel: tunnel=%v error=%v", got.tunnel, got.err)
			}
		})
	}
}

func TestProviderWaitsForRegisteredEdge(t *testing.T) {
	binary, announced, release := gatedProviderFixture(t, "echo https://ready-test.trycloudflare.com >&2",
		"echo 'Registered tunnel connection connIndex=0 protocol=http2' >&2\nwhile :; do sleep 0.05; done")
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	result := startProviderForTest(ctx, binary)
	waitProviderAnnouncement(t, ctx, announced, result)
	assertProviderAwaitingReady(t, ctx, result)
	if err := os.WriteFile(release, nil, 0600); err != nil {
		t.Fatal(err)
	}
	got := awaitProviderResult(t, ctx, result)
	if got.err != nil {
		t.Fatal(got.err)
	}
	if got.tunnel == nil {
		t.Fatal("registered provider did not return a tunnel")
	}
	defer got.tunnel.Close()
	if got.tunnel.URL != "https://ready-test.trycloudflare.com" {
		t.Fatal("wrong allocated tunnel URL")
	}
}

func TestProviderCancellationWhileAwaitingRegisteredEdge(t *testing.T) {
	binary, announced, _ := gatedProviderFixture(t, "echo https://unready.trycloudflare.com >&2", "exit 1")
	watchdog, stop := context.WithTimeout(context.Background(), 10*time.Second)
	defer stop()
	ctx, cancel := context.WithCancel(watchdog)
	defer cancel()
	result := startProviderForTest(ctx, binary)
	waitProviderAnnouncement(t, watchdog, announced, result)
	assertProviderAwaitingReady(t, watchdog, result)
	cancel()
	got := awaitProviderResult(t, watchdog, result)
	if got.tunnel != nil {
		defer got.tunnel.Close()
	}
	// Cancel also kills the process. Start may observe that exit before its
	// context select case; both paths must return an error and no tunnel.
	cancelError := errors.Is(got.err, context.Canceled) || got.err != nil && strings.Contains(got.err.Error(), "before ready")
	if got.tunnel != nil || ctx.Err() != context.Canceled || !cancelError {
		t.Fatalf("cancellation returned tunnel=%v error=%v", got.tunnel, got.err)
	}
}

func TestFallbackRefreshesProviderMetadata(t *testing.T) {
	host, dialer, ctx := testClients(t, true)
	host.mu.Lock()
	saved := host.tunnelURL
	host.tunnelURL = ""
	host.mu.Unlock()
	if err := host.heartbeat(ctx); err != nil {
		t.Fatal(err)
	}
	ln := echoListener(t, false)
	if err := host.Expose(ctx, Port{ID: "echo", Port: ln.Addr().(*net.TCPAddr).Port, Type: "echo", Transport: "tcp"}); err != nil {
		t.Fatal(err)
	}
	published := make(chan error, 1)
	go func() {
		time.Sleep(100 * time.Millisecond)
		host.mu.Lock()
		host.tunnelURL = saved
		host.mu.Unlock()
		published <- host.heartbeat(ctx)
	}()
	m, err := dialer.Map(ctx, host.Session().DeviceID, "echo", "")
	if err != nil {
		t.Fatal(err)
	}
	defer m.Close()
	if err = <-published; err != nil {
		t.Fatal(err)
	}
	if m.Transport != "cloudflare-tunnel" {
		t.Fatalf("transport %q", m.Transport)
	}
}
func TestExpiryCancelsActiveMapping(t *testing.T) {
	host, dialer, ctx := testClients(t, true)
	ln := echoListener(t, false)
	if err := host.Expose(ctx, Port{ID: "echo", Port: ln.Addr().(*net.TCPAddr).Port, Type: "echo", Transport: "tcp"}); err != nil {
		t.Fatal(err)
	}
	m, err := dialer.Map(ctx, host.Session().DeviceID, "echo", "")
	if err != nil {
		t.Fatal(err)
	}
	defer m.Close()
	dialer.mu.Lock()
	dialer.session.ExpiresAt = time.Now().Add(25 * time.Millisecond)
	dialer.mu.Unlock()
	select {
	case <-dialer.ctx.Done():
	case <-time.After(time.Second):
		t.Fatal("expiry did not cancel client")
	}
	select {
	case <-m.Peer.session.CloseChan():
	case <-time.After(time.Second):
		t.Fatal("expiry did not close peer")
	}
}
func TestAPIBodyLimit(t *testing.T) {
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = w.Write(bytes.Repeat([]byte("x"), (2<<20)+1)) }))
	defer s.Close()
	c, err := New(Config{BaseURL: s.URL, AppID: "a"})
	if err != nil {
		t.Fatal(err)
	}
	if _, err = c.AuthConfig(context.Background()); err == nil || !strings.Contains(err.Error(), "exceeds") {
		t.Fatalf("expected explicit body limit, got %v", err)
	}
}

// A blackholed TLS peer must not make cancellation wait for close_notify.
func TestCloseUnblocksStalledTLS(t *testing.T) {
	left, right := net.Pipe()
	defer left.Close()
	defer right.Close()
	cert, _, err := newCertificate("test")
	if err != nil {
		t.Fatal(err)
	}
	clientTLS := tls.Client(left, &tls.Config{InsecureSkipVerify: true, MinVersion: tls.VersionTLS13})
	serverTLS := tls.Server(right, &tls.Config{Certificates: []tls.Certificate{cert}, MinVersion: tls.VersionTLS13, SessionTicketsDisabled: true})
	serverDone := make(chan error, 1)
	go func() { serverDone <- serverTLS.Handshake() }()
	if err = clientTLS.Handshake(); err != nil {
		t.Fatal(err)
	}
	if err = <-serverDone; err != nil {
		t.Fatal(err)
	}
	session, err := smux.Client(clientTLS, nil)
	if err != nil {
		t.Fatal(err)
	}
	c := &Client{peers: map[string]*Peer{}}
	p := &Peer{client: c, session: session, raw: left}
	done := make(chan struct{})
	go func() { p.Close(); close(done) }()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("Close blocked waiting for peer TLS traffic")
	}
}

func TestRememberPeerAfterCancellationClosesImmediately(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	left, right := net.Pipe()
	defer left.Close()
	defer right.Close()
	session, err := smux.Client(left, nil)
	if err != nil {
		t.Fatal(err)
	}
	c := &Client{ctx: ctx, peers: map[string]*Peer{}}
	p := c.rememberPeer("cancelled-peer", "test", session, left)
	select {
	case <-p.session.CloseChan():
	case <-time.After(time.Second):
		t.Fatal("peer created after cancellation remained open")
	}
	p.Close()
}
