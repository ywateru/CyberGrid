package cybergrid

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

// Cloudflared starts a temporary anonymous tunnel to the SDK's authenticated
// WebSocket origin. It never points directly at an exposed application port.
type Cloudflared struct {
	// Binary is an explicit trusted external override; empty selects the bundled provider.
	Binary string
	// CacheDir is an absolute cache base; macOS defaults to the OS user cache.
	CacheDir string
	// NativeLibraryDir is Android ApplicationInfo.nativeLibraryDir; only the fixed installed resource is used.
	NativeLibraryDir string
	TempDir          string
	Env              map[string]string
	// EnvProvider re-reads current non-secret platform settings before each spawn.
	EnvProvider func() (map[string]string, error)
}

// cloudflared prints the allocated hostname as a standalone URL inside its
// banner. Never extract a hostname from an error's URL, such as api/.../tunnel.
var tunnelPattern = regexp.MustCompile(`(?:^|\|)[[:space:]]*(https://[a-z0-9-]+\.trycloudflare\.com)[[:space:]]*(?:\||$)`)

func allocatedTunnelURL(line string) (string, bool) {
	match := tunnelPattern.FindStringSubmatch(line)
	if len(match) != 2 || match[1] == "https://api.trycloudflare.com" || ValidateTunnelURL(match[1]) != nil {
		return "", false
	}
	return match[1], true
}

func ValidateTunnelURL(value string) error {
	u, err := url.Parse(value)
	if err != nil {
		return errors.New("invalid tunnel URL")
	}
	h := strings.ToLower(u.Hostname())
	label := strings.TrimSuffix(h, ".trycloudflare.com")
	if u.Scheme != "https" || u.User != nil || u.Port() != "" || !strings.HasSuffix(h, ".trycloudflare.com") || label == "" || strings.Contains(label, ".") || u.RawQuery != "" || u.Fragment != "" || (u.Path != "" && u.Path != "/") {
		return errors.New("tunnel URL must be a root https://<name>.trycloudflare.com URL")
	}
	return nil
}
func (p Cloudflared) Start(ctx context.Context, origin string) (*Tunnel, error) {
	path, err := p.executable(ctx)
	if err != nil {
		return nil, fmt.Errorf("Quick Tunnel provider resolution failed: %w", err)
	}
	settings := make(map[string]string)
	for k, v := range p.Env {
		settings[k] = v
	}
	if p.EnvProvider != nil {
		current, e := p.EnvProvider()
		if e != nil {
			return nil, errors.New("provider environment unavailable")
		}
		for k, v := range current {
			settings[k] = v
		}
	}
	// An explicit empty config avoids inheriting a named-tunnel config.
	dir, err := os.MkdirTemp(p.TempDir, "cybergrid-cloudflared-")
	if err != nil {
		return nil, err
	}
	configPath := filepath.Join(dir, "config.yml")
	if err = os.WriteFile(configPath, []byte("{}\n"), 0600); err != nil {
		os.RemoveAll(dir)
		return nil, err
	}
	child, cancel := context.WithCancel(ctx)
	cmd := exec.CommandContext(child, path, "tunnel", "--config", configPath, "--no-autoupdate", "--url", origin, "--protocol", "http2", "--metrics", "127.0.0.1:0")
	// The tunnel subprocess needs no CyberGrid account/session credentials.
	cmd.Env = make([]string, 0)
	allowed := map[string]bool{"PATH": true, "HOME": true, "TMPDIR": true, "LANG": true, "LC_ALL": true, "ANDROID_ROOT": true, "ANDROID_DATA": true, "LD_LIBRARY_PATH": true, "SSL_CERT_FILE": true, "SSL_CERT_DIR": true, "CLOUDFLARED_DNS_SERVER": true}
	for key := range allowed {
		if value, ok := os.LookupEnv(key); ok {
			cmd.Env = append(cmd.Env, key+"="+value)
		}
	}
	for key, value := range settings {
		if !allowed[key] {
			cancel()
			os.RemoveAll(dir)
			return nil, errors.New("provider environment key is not allowed")
		}
		cmd.Env = append(cmd.Env, key+"="+value)
	}
	cmd.Dir = dir
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		cancel()
		os.RemoveAll(dir)
		return nil, err
	}
	cmd.Stderr = cmd.Stdout
	if err = cmd.Start(); err != nil {
		cancel()
		os.RemoveAll(dir)
		return nil, err
	}
	found := make(chan string, 1)
	ready := make(chan struct{})
	done := make(chan error, 1)
	exited := make(chan struct{})
	go func() {
		var readyOnce sync.Once
		scanner := bufio.NewScanner(stdout)
		scanner.Buffer(make([]byte, 4096), 1<<20)
		for scanner.Scan() {
			line := scanner.Text()
			if v, ok := allocatedTunnelURL(line); ok {
				select {
				case found <- v:
				default:
				}
			}
			if strings.Contains(line, "Registered tunnel connection") {
				readyOnce.Do(func() { close(ready) })
			}
		}
		done <- cmd.Wait()
		close(exited)
		_ = os.RemoveAll(dir)
	}()
	var once sync.Once
	closeFn := func() error {
		once.Do(cancel)
		select {
		case <-exited:
			return nil
		case <-time.After(5 * time.Second):
			return errors.New("cloudflared exit timed out")
		}
	}
	startup := time.NewTimer(30 * time.Second)
	defer startup.Stop()
	var value string
	registered := false
	readyEvent := (<-chan struct{})(ready)
	for {
		select {
		case value = <-found:
		case <-readyEvent:
			registered = true
			readyEvent = nil
		case err = <-done:
			cancel()
			return nil, fmt.Errorf("cloudflared exited before ready (URL allocated=%t, edge registered=%t): %v", value != "", registered, err)
		case <-startup.C:
			closeFn()
			return nil, fmt.Errorf("cloudflared startup timed out after 30 seconds (URL allocated=%t, edge registered=%t)", value != "", registered)
		case <-ctx.Done():
			closeFn()
			return nil, ctx.Err()
		}
		if value != "" && registered {
			return &Tunnel{URL: value, Close: closeFn, Done: done}, nil
		}
	}
}
func (c *Client) startOrigin() error {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return err
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/connect", c.acceptWebSocket)
	srv := &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second, MaxHeaderBytes: 8192}
	c.mu.Lock()
	if c.closing || c.ctx == nil || c.ctx.Err() != nil {
		c.mu.Unlock()
		ln.Close()
		return context.Canceled
	}
	c.origin = ln
	c.server = srv
	c.wg.Add(1)
	c.mu.Unlock()
	go func() {
		defer c.wg.Done()
		if err := srv.Serve(ln); err != nil && !errors.Is(err, http.ErrServerClosed) {
			c.report(err)
		}
	}()
	return nil
}
func (c *Client) startTunnel() {
	c.mu.Lock()
	ctx := c.ctx
	c.mu.Unlock()
	c.startTunnelGeneration(ctx)
}
func (c *Client) startTunnelGeneration(ctx context.Context) {
	defer c.wg.Done()
	provider := c.cfg.TunnelProvider
	if provider == nil {
		provider = Cloudflared{}
	}
	c.mu.Lock()
	if c.closing || ctx == nil || ctx.Err() != nil || c.ctx != ctx || c.origin == nil {
		c.mu.Unlock()
		return
	}
	origin := "http://" + c.origin.Addr().String()
	memory := c.memory
	c.mu.Unlock()
	for attempt := 0; attempt < 6; attempt++ {
		if ctx.Err() != nil {
			return
		}
		if attempt > 0 {
			c.coordinationEvent("provider-restarting", "")
			select {
			case <-ctx.Done():
				return
			case <-time.After(retryDelay(attempt-1, nil)):
			}
		}
		t, err := provider.Start(ctx, origin)
		if err == nil && t == nil {
			err = errors.New("tunnel provider returned no tunnel")
		}
		if err == nil {
			err = ValidateTunnelURL(t.URL)
		}
		if err != nil {
			if t != nil && t.Close != nil {
				t.Close()
			}
			c.mu.Lock()
			if c.ctx == ctx {
				c.tunnelErr = err
			}
			c.mu.Unlock()
			if ctx.Err() == nil {
				c.report(err)
				c.coordinationEvent("provider-failed", "start-failed")
			}
			if memory {
				continue
			}
			return
		}
		c.publishMu.Lock()
		c.mu.Lock()
		if ctx.Err() != nil || c.ctx != ctx || c.closing {
			c.mu.Unlock()
			c.publishMu.Unlock()
			t.Close()
			return
		}
		c.tunnel = t
		c.tunnelURL = t.URL
		c.mu.Unlock()
		err = c.heartbeatLocked(ctx)
		c.publishMu.Unlock()
		if err != nil {
			c.report(err)
		}
		if t.Done == nil {
			<-ctx.Done()
			return
		}
		select {
		case err = <-t.Done:
		case <-ctx.Done():
			return
		}
		c.publishMu.Lock()
		c.mu.Lock()
		if c.ctx != ctx {
			c.mu.Unlock()
			c.publishMu.Unlock()
			return
		}
		c.tunnelURL = ""
		c.tunnel = nil
		c.tunnelErr = errors.New("tunnel provider exited")
		c.mu.Unlock()
		if ctx.Err() == nil {
			if e := c.heartbeatLocked(ctx); e != nil {
				c.report(e)
			}
		}
		c.publishMu.Unlock()
		if ctx.Err() != nil {
			return
		}
		if err != nil {
			c.report(errors.New("tunnel provider exited"))
		}
		c.coordinationEvent("provider-failed", "exited")
		if !memory {
			return
		}
	}
}
func (c *Client) acceptWebSocket(w http.ResponseWriter, r *http.Request) {
	if r.Method != "GET" || r.URL.RawQuery != "" {
		http.Error(w, "invalid request", 400)
		return
	}
	c.mu.Lock()
	ctx := c.ctx
	c.mu.Unlock()
	if ctx == nil || ctx.Err() != nil {
		http.Error(w, "session unavailable", 503)
		return
	}
	c.event("websocket-request", "cloudflare-tunnel", "")
	up := websocket.Upgrader{HandshakeTimeout: 5 * time.Second, CheckOrigin: func(r *http.Request) bool { return r.Header.Get("Origin") == "" }, Subprotocols: []string{"cybergrid-v1"}}
	ws, err := up.Upgrade(w, r, nil)
	if err != nil {
		c.report(fmt.Errorf("WebSocket origin upgrade: %w", err))
		return
	}
	c.event("websocket-upgraded", "cloudflare-tunnel", "")
	ws.SetReadLimit(1 << 20)
	raw := newWSConn(ws)
	handshake, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	stop := context.AfterFunc(handshake, func() { raw.Close() })
	_, err = c.secureServer(handshake, raw, "cloudflare-tunnel")
	stop()
	if err != nil {
		raw.Close()
		if ctx.Err() == nil {
			c.report(fmt.Errorf("tunnel authentication: %w", err))
		}
	}
}
func (c *Client) dialTunnel(ctx context.Context, d Device) (*Peer, error) {
	if d.TunnelURL == nil || *d.TunnelURL == "" {
		return nil, errors.New("direct connection failed and remote device has no Quick Tunnel URL; remote provider may be missing or not ready")
	}
	if err := ValidateTunnelURL(*d.TunnelURL); err != nil {
		return nil, err
	}
	endpoint := strings.TrimRight(*d.TunnelURL, "/") + "/connect"
	c.mu.Lock()
	test := c.testTunnelURL
	c.mu.Unlock()
	if test != "" {
		endpoint = test + "/connect"
	}
	u, err := url.Parse(endpoint)
	if err != nil {
		return nil, err
	}
	if u.Scheme == "https" {
		u.Scheme = "wss"
	} else {
		u.Scheme = "ws"
	}
	c.event("websocket-dial", "cloudflare-tunnel", d.ID)
	dialer := websocket.Dialer{HandshakeTimeout: 10 * time.Second, Proxy: http.ProxyFromEnvironment, Subprotocols: []string{"cybergrid-v1"}}
	ws, resp, err := dialer.DialContext(ctx, u.String(), nil)
	if err != nil {
		if resp != nil && resp.Body != nil {
			resp.Body.Close()
		}
		return nil, fmt.Errorf("WebSocket upgrade: %w", err)
	}
	c.event("websocket-upgraded", "cloudflare-tunnel", d.ID)
	ws.SetReadLimit(1 << 20)
	raw := newWSConn(ws)
	stop := context.AfterFunc(ctx, func() { raw.Close() })
	peer, err := c.secureClient(ctx, raw, d, "cloudflare-tunnel")
	stop()
	if err != nil {
		raw.Close()
		return nil, fmt.Errorf("end-to-end TLS authentication: %w", err)
	}
	c.event("mutual-tls-ready", "cloudflare-tunnel", d.ID)
	return peer, nil
}

type wsConn struct {
	ws      *websocket.Conn
	reader  io.Reader
	writeMu sync.Mutex
}

func newWSConn(ws *websocket.Conn) net.Conn { return &wsConn{ws: ws} }
func (w *wsConn) Read(p []byte) (int, error) {
	for {
		if w.reader == nil {
			kind, r, err := w.ws.NextReader()
			if err != nil {
				return 0, err
			}
			if kind != websocket.BinaryMessage {
				return 0, errors.New("expected binary WebSocket frame")
			}
			w.reader = r
		}
		n, err := w.reader.Read(p)
		if err == io.EOF {
			w.reader = nil
			if n > 0 {
				return n, nil
			}
			continue
		}
		return n, err
	}
}
func (w *wsConn) Write(p []byte) (int, error) {
	w.writeMu.Lock()
	defer w.writeMu.Unlock()
	if err := w.ws.WriteMessage(websocket.BinaryMessage, p); err != nil {
		return 0, err
	}
	return len(p), nil
}
func (w *wsConn) Close() error         { return w.ws.Close() }
func (w *wsConn) LocalAddr() net.Addr  { return w.ws.LocalAddr() }
func (w *wsConn) RemoteAddr() net.Addr { return w.ws.RemoteAddr() }
func (w *wsConn) SetDeadline(t time.Time) error {
	if err := w.ws.SetReadDeadline(t); err != nil {
		return err
	}
	return w.ws.SetWriteDeadline(t)
}
func (w *wsConn) SetReadDeadline(t time.Time) error  { return w.ws.SetReadDeadline(t) }
func (w *wsConn) SetWriteDeadline(t time.Time) error { return w.ws.SetWriteDeadline(t) }
