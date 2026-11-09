package cybergrid

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"testing"
	"time"
)

// Explicit test identity only. The coordination state and request handler below
// are the actual TypeScript product, in an independently killable Node process.
func startMemoryOwner(t *testing.T, port int, drop bool) (string, string, func()) {
	t.Helper()
	server, e := filepath.Abs("../server")
	if e != nil {
		t.Fatal(e)
	}
	loader := filepath.Join(server, "node_modules/tsx/dist/loader.mjs")
	if _, e = os.Stat(loader); e != nil {
		t.Skip("repository Node fixture dependency unavailable; run server npm ci for coordinator integration")
	}
	script := fmt.Sprintf(`import{createServer}from'node:http';import{createService}from%q;import{MemoryStore}from%q;import{serveAPI}from%q;
 const store=new MemoryStore();const identity={configuration:(clientId)=>({provider:'logto',issuer:'https://fixture.invalid/oidc',clientId,resource:'https://cybergrid.aixgg.com/api',scope:'openid offline_access connect',deviceAuthorizationEndpoint:'https://fixture.invalid/oidc/device/auth',tokenEndpoint:'https://fixture.invalid/oidc/token'}),verify:async(token)=>{if(token!=='explicit-memory-fixture')throw Error('fixture denied');return{id:'fixed-fixture-user',clientId:'NativeCaseA'};}};
 const handle=createService({store,identity,directory:{list:async()=>[{id:'NativeCaseA',name:'Fixture',type:'Native',isDeviceFlow:false}]},browserClientId:'ConsoleSPA',authRateLimit:1000});let dropRegister=%t,dropPublish=%t;
 const wrapped=async(req,ip)=>{const response=await handle(req,ip);if(response.ok&&((req.url.endsWith('/auth/exchange')&&dropRegister)||(req.url.endsWith('/devices/self')&&dropPublish))){if(req.url.endsWith('/auth/exchange'))dropRegister=false;else dropPublish=false;return new Response('drop',{status:599,headers:{'x-cybergrid-epoch':store.epoch}});}return response;};
 const srv=createServer((req,res)=>{const write=res.writeHead.bind(res);res.writeHead=(status,...args)=>{if(status===599){res.destroy();return res;}return write(status,...args)};void serveAPI(req,res,wrapped);});srv.listen(%d,'127.0.0.1',()=>console.log(JSON.stringify({port:srv.address().port,epoch:store.epoch})));`, filepath.Join(server, "src/service.ts"), filepath.Join(server, "src/memory.ts"), filepath.Join(server, "src/http.ts"), drop, drop, port)
	path := filepath.Join(t.TempDir(), "owner.mts")
	if e = os.WriteFile(path, []byte(script), 0600); e != nil {
		t.Fatal(e)
	}
	cmd := exec.Command("node", "--import", loader, path)
	out, e := cmd.StdoutPipe()
	if e != nil {
		t.Fatal(e)
	}
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if e = cmd.Start(); e != nil {
		t.Fatal(e)
	}
	ready := make(chan string, 1)
	go func() {
		scanner := bufio.NewScanner(out)
		if scanner.Scan() {
			ready <- scanner.Text()
		} else {
			ready <- ""
		}
	}()
	var line string
	select {
	case line = <-ready:
	case <-time.After(10 * time.Second):
		cmd.Process.Kill()
		t.Fatal("owner startup timeout")
	}
	var data struct {
		Port  int    `json:"port"`
		Epoch string `json:"epoch"`
	}
	if e = json.Unmarshal([]byte(line), &data); e != nil {
		cmd.Process.Kill()
		t.Fatalf("owner startup: %s %v", stderr.String(), e)
	}
	closed := false
	stop := func() {
		if !closed {
			closed = true
			cmd.Process.Kill()
			cmd.Wait()
		}
	}
	t.Cleanup(stop)
	return "http://127.0.0.1:" + strconv.Itoa(data.Port), data.Epoch, stop
}
func waitMemory(t *testing.T, deadline time.Time, f func() bool) {
	t.Helper()
	for time.Now().Before(deadline) {
		if f() {
			return
		}
		time.Sleep(100 * time.Millisecond)
	}
	t.Fatal("memory coordination recovery timeout")
}
func memoryProbe(t *testing.T, addr string) {
	t.Helper()
	conn, e := net.DialTimeout("tcp", addr, time.Second)
	if e != nil {
		t.Fatal(e)
	}
	defer conn.Close()
	conn.SetDeadline(time.Now().Add(10 * time.Second))
	data := bytes.Repeat([]byte("memory-verified!"), 4096)
	done := make(chan error, 1)
	go func() {
		_, err := conn.Write(data)
		if err == nil {
			err = conn.(*net.TCPConn).CloseWrite()
		}
		done <- err
	}()
	out, e := io.ReadAll(conn)
	if e != nil {
		t.Fatal(e)
	}
	if e = <-done; e != nil {
		t.Fatal(e)
	}
	if !bytes.Equal(append([]byte("ack:"), data...), out) {
		t.Fatalf("echo mismatch %d != %d", len(out), len(data))
	}
}
func TestMemoryActualOwnerRestartLostRegistrationAndFirstSnapshot(t *testing.T) {
	base, epoch, stop := startMemoryOwner(t, 0, true)
	ctx, cancel := context.WithTimeout(context.Background(), 75*time.Second)
	defer cancel()
	provider := AuthProviderFunc(func(context.Context, *Client) (OAuthTokens, error) {
		return OAuthTokens{AccessToken: "explicit-memory-fixture", ExpiresAt: time.Now().Add(time.Hour)}, nil
	})
	newClient := func(name string) *Client {
		c, e := New(Config{BaseURL: base, AppID: "NativeCaseA", AuthProvider: provider, DisableTunnel: true, STUNServers: []string{}, DirectTimeout: 3 * time.Second})
		if e != nil {
			t.Fatal(e)
		}
		t.Cleanup(func() { c.Close() })
		if _, e = c.LoginWithAccessToken(ctx, "explicit-memory-fixture", name); e != nil {
			t.Fatal(e)
		}
		return c
	}
	host, dial := newClient("host"), newClient("dial")
	oldHost, oldDial := host.Session(), dial.Session()
	ln := echoListener(t, true)
	if e := host.Expose(ctx, Port{ID: "echo", Port: ln.Addr().(*net.TCPAddr).Port, Type: "echo", Transport: "tcp"}); e != nil {
		t.Fatal(e)
	}
	mapping, e := dial.Map(ctx, oldHost.DeviceID, "echo", "127.0.0.1:0")
	if e != nil {
		t.Fatal(e)
	}
	defer mapping.Close()
	memoryProbe(t, mapping.Addr.String())
	stop()
	_, portString, _ := net.SplitHostPort(base[len("http://"):])
	port, _ := strconv.Atoi(portString)
	_, nextEpoch, stop2 := startMemoryOwner(t, port, false)
	defer stop2()
	if nextEpoch == epoch {
		t.Fatal("boot epoch reused")
	}
	waitMemory(t, time.Now().Add(25*time.Second), func() bool {
		h, d := host.Session(), dial.Session()
		host.mu.Lock()
		hr := host.ready
		host.mu.Unlock()
		dial.mu.Lock()
		dr := dial.ready
		dial.mu.Unlock()
		return hr && dr && h.CoordinatorEpoch == nextEpoch && d.CoordinatorEpoch == nextEpoch
	})
	if host.Session().DeviceID != oldHost.DeviceID || dial.Session().DeviceID != oldDial.DeviceID {
		t.Fatal("logical registration identity changed")
	}
	if host.Session().Incarnation == oldHost.Incarnation {
		t.Fatal("old incarnation revived")
	}
	waitMemory(t, time.Now().Add(5*time.Second), func() bool {
		ds, e := dial.Devices(ctx)
		if e != nil {
			return false
		}
		for _, d := range ds {
			if d.ID == oldHost.DeviceID {
				return len(d.Ports) == 1 && d.Online
			}
		}
		return false
	})
	memoryProbe(t, mapping.Addr.String())
}
