// cybergrid is a native SDK test client. ADB may launch it and collect logs;
// application bytes always use the selected peer transport.
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	cybergrid "github.com/ywateru/CyberGrid/sdk"
)

var outputMu sync.Mutex

func output(v any) {
	outputMu.Lock()
	defer outputMu.Unlock()
	_ = json.NewEncoder(os.Stdout).Encode(v)
}
func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
func run() error {
	if len(os.Args) < 2 {
		return errors.New("usage: cybergrid login|logout|apps|devices|host|map|probe|echo|version [flags]")
	}
	command := os.Args[1]
	if command == "version" {
		output(map[string]string{"version": cybergrid.Version})
		return nil
	}
	switch command {
	case "login", "logout", "apps", "devices", "host", "map", "probe", "echo":
	default:
		return fmt.Errorf("unsupported command %q; use login for hosted Logto registration and sign-in", command)
	}
	f := flag.NewFlagSet(command, flag.ContinueOnError)
	base := f.String("server", envDefault("CYBERGRID_SERVER", "https://cybergrid.aixgg.com"), "central HTTPS URL (loopback HTTP permitted for development)")
	app := f.String("app", os.Getenv("CYBERGRID_APP_ID"), "required case-sensitive Logto Native application ID (public client_id)")
	name := f.String("name", "", "device name")
	creds := f.String("credentials", os.Getenv("CYBERGRID_CREDENTIALS"), "private version 4 Logto refresh credential file (one grant per device)")
	openBrowser := f.Bool("open-browser", false, "login: open the verification URL in the macOS default browser")
	peer := f.String("peer", "", "remote device ID")
	port := f.String("port", "echo", "remote exposed port ID")
	local := f.String("local", "127.0.0.1:0", "loopback mapping address")
	echoPort := f.Int("echo-port", -1, "host: start echo service on this loopback port; 0 assigns a port")
	expose := f.String("expose", "", "host: id:port:type (single TCP service)")
	stun := f.String("stun", "", "comma-separated STUN URLs, or none to disable public STUN")
	iface := f.String("interface", "", "restrict ICE to network interface, e.g. en0 or rmnet_data0")
	directTimeout := f.Duration("direct-timeout", 12*time.Second, "direct attempt timeout before tunnel fallback")
	fallbackTimeout := f.Duration("fallback-timeout", 35*time.Second, "provider readiness plus tunnel authentication timeout")
	diagnostics := f.Bool("diagnostics", false, "print transport phases and probe byte progress (no credentials or payloads)")
	noTunnel := f.Bool("no-tunnel", false, "do not start a local tunnel provider")
	providerCache := f.String("provider-cache", "", "absolute provider cache base (default OS user cache)")
	nativeDir := f.String("native-library-dir", "", "Android installed ApplicationInfo.nativeLibraryDir")
	cf := f.String("cloudflared", "", "explicit trusted external cloudflared override (default: bundled provider)")
	duration := f.Duration("duration", 0, "exit after this duration; 0 waits for signal")
	probeTimeout := f.Duration("probe-timeout", 45*time.Second, "probe TCP transfer deadline (does not alter SDK connection budgets)")
	size := f.Int("bytes", 1048576, "probe: byte count, 1..67108864")
	listen := f.String("listen", "127.0.0.1:0", "echo server listen address")
	if err := f.Parse(os.Args[2:]); err != nil {
		return err
	}
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()
	if *duration > 0 {
		var stop context.CancelFunc
		ctx, stop = context.WithTimeout(ctx, *duration)
		defer stop()
	}
	if command == "echo" {
		ln, err := startEcho(ctx, *listen)
		if err != nil {
			return err
		}
		defer ln.Close()
		output(map[string]any{"event": "echo", "address": ln.Addr().String()})
		<-ctx.Done()
		return nil
	}
	cfg := cybergrid.Config{BaseURL: *base, AppID: *app, DirectTimeout: *directTimeout, FallbackTimeout: *fallbackTimeout, DisableTunnel: *noTunnel || command == "apps", InterfaceName: *iface, TunnelProvider: cybergrid.Cloudflared{Binary: *cf, CacheDir: *providerCache, NativeLibraryDir: *nativeDir}, OnError: func(err error) { fmt.Fprintln(os.Stderr, "SDK:", err) }}
	cfg.OnTransportEvent = func(event cybergrid.TransportEvent) {
		if *diagnostics || event.Generation != 0 {
			output(map[string]any{"event": "transport-stage", "detail": event})
		}
		if event.Phase == "requires-login" {
			cancel()
		}
	}
	if *stun == "none" {
		cfg.STUNServers = []string{}
	} else if *stun != "" {
		cfg.STUNServers = strings.Split(*stun, ",")
	}
	c, err := cybergrid.New(cfg)
	if err != nil {
		return err
	}
	defer func() { c.Close(); preservePending(c) }()
	c.SetGrantCheckpoint(func(ctx context.Context, client *cybergrid.Client, t cybergrid.OAuthTokens) error {
		a, ok := client.CachedAuthConfig()
		if !ok {
			return errors.New("verified auth binding missing")
		}
		return writeCredentials(*creds, newCredentials(*base, a, t.RefreshToken))
	})
	c.SetAuthProvider(cybergrid.AuthProviderFunc(func(authCtx context.Context, client *cybergrid.Client) (cybergrid.OAuthTokens, error) {
		return refreshCredentials(authCtx, client, *base, *app, *creds)
	}))

	if command == "login" {
		return login(ctx, c, *base, *app, *creds, *openBrowser)
	}
	if command == "logout" {
		return logout(ctx, c, *base, *app, *creds)
	}
	if *name == "" {
		*name, _ = os.Hostname()
	}
	tokens, err := refreshCredentials(ctx, c, *base, *app, *creds)
	if err != nil {
		return err
	}
	session, err := c.LoginWithAccessToken(ctx, tokens.AccessToken, *name)
	tokens = cybergrid.OAuthTokens{}
	if err != nil {
		return err
	}
	output(map[string]any{"event": "login", "deviceId": session.DeviceID, "appId": *app, "expiresAt": session.ExpiresAt})
	switch command {
	case "apps":
		apps, err := c.Apps(ctx)
		if err != nil {
			return err
		}
		output(map[string]any{"source": "logto", "apps": apps, "note": "Read-only tenant directory, not a list of this user's authorizations."})
		return nil
	case "devices":
		ds, err := c.Devices(ctx)
		if err != nil {
			return err
		}
		output(map[string]any{"devices": ds})
		return nil
	case "host":
		if *echoPort >= 0 {
			ln, err := startEcho(ctx, net.JoinHostPort("127.0.0.1", strconv.Itoa(*echoPort)))
			if err != nil {
				return err
			}
			defer ln.Close()
			p := ln.Addr().(*net.TCPAddr).Port
			if err = c.Expose(ctx, cybergrid.Port{ID: "echo", Port: p, Type: "echo", Transport: "tcp"}); err != nil {
				return err
			}
			output(map[string]any{"event": "exposed", "id": "echo", "port": p, "type": "echo"})
		}
		if *expose != "" {
			parts := strings.SplitN(*expose, ":", 3)
			if len(parts) != 3 {
				return errors.New("expose must be id:port:type")
			}
			n, err := strconv.Atoi(parts[1])
			if err != nil {
				return err
			}
			if err = c.Expose(ctx, cybergrid.Port{ID: parts[0], Port: n, Type: parts[2], Transport: "tcp"}); err != nil {
				return err
			}
			output(map[string]any{"event": "exposed", "id": parts[0], "port": n, "type": parts[2]})
		}
		output(map[string]any{"event": "host-ready", "deviceId": session.DeviceID})
		<-ctx.Done()
		return nil
	case "map", "probe":
		if *peer == "" {
			return errors.New("-peer device ID is required")
		}
		m, err := c.Map(ctx, *peer, *port, *local)
		if err != nil {
			return err
		}
		defer m.Close()
		output(map[string]any{"event": "mapped", "address": m.Addr.String(), "transport": m.Transport, "peer": *peer, "portId": *port, "path": m.Peer.SelectedPath()})
		if command == "map" {
			<-ctx.Done()
			return nil
		}
		if *probeTimeout <= 0 {
			return errors.New("probe timeout must be positive")
		}
		if *size < 1 || *size > 64<<20 {
			return errors.New("bytes must be 1..67108864")
		}
		conn, err := net.DialTimeout("tcp", m.Addr.String(), 5*time.Second)
		if err != nil {
			return err
		}
		defer conn.Close()
		if *diagnostics {
			instrumented := &observedConn{Conn: conn}
			conn = instrumented
			finished := make(chan struct{})
			defer close(finished)
			go func() {
				ticker := time.NewTicker(2 * time.Second)
				defer ticker.Stop()
				for {
					select {
					case <-finished:
						return
					case <-ticker.C:
						output(map[string]any{"event": "probe-progress", "written": instrumented.written.Load(), "read": instrumented.read.Load()})
					}
				}
			}()
		}
		_ = conn.SetDeadline(time.Now().Add(*probeTimeout))
		buf := make([]byte, *size)
		for i := range buf {
			buf[i] = byte((i*31 + i/257) % 251)
		}
		expected := sha256.Sum256(buf)
		sent := make(chan error, 1)
		start := time.Now()
		go func() { _, err := io.Copy(conn, strings.NewReader(string(buf))); sent <- err }()
		hash := sha256.New()
		n, err := io.CopyN(hash, conn, int64(len(buf)))
		if err != nil {
			return err
		}
		if err = <-sent; err != nil {
			return err
		}
		actual := hex.EncodeToString(hash.Sum(nil))
		want := hex.EncodeToString(expected[:])
		if actual != want {
			return errors.New("echo payload SHA256 mismatch")
		}
		output(map[string]any{"event": "verified", "transport": m.Transport, "bytes": n, "sha256": actual, "seconds": time.Since(start).Seconds()})
		return nil
	default:
		return fmt.Errorf("unknown command %q", command)
	}
}
func startEcho(ctx context.Context, address string) (net.Listener, error) {
	ln, err := net.Listen("tcp", address)
	if err != nil {
		return nil, err
	}
	context.AfterFunc(ctx, func() { ln.Close() })
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go func() {
				defer conn.Close()
				stop := context.AfterFunc(ctx, func() { conn.Close() })
				defer stop()
				_, _ = io.Copy(conn, conn)
			}()
		}
	}()
	return ln, nil
}

type observedConn struct {
	net.Conn
	read, written atomic.Int64
}

func (c *observedConn) Read(p []byte) (int, error) {
	n, err := c.Conn.Read(p)
	c.read.Add(int64(n))
	return n, err
}
func (c *observedConn) Write(p []byte) (int, error) {
	n, err := c.Conn.Write(p)
	c.written.Add(int64(n))
	return n, err
}
