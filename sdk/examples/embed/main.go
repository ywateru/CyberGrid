// embed demonstrates a private in-process grant owner and automatic recovery.
// For durable authorization across restarts use an OS-backed private vault;
// the CLI implements the version4 locked-file contract separately.
package main

import (
	"context"
	"fmt"
	cybergrid "github.com/ywateru/CyberGrid/sdk"
	"log"
	"os"
	"os/signal"
	"sync"
	"syscall"
	"time"
)

func main() {
	if err := run(); err != nil {
		log.Print(err)
		os.Exit(1)
	}
}
func run() error {
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()
	c, err := cybergrid.New(cybergrid.Config{BaseURL: "https://cybergrid.aixgg.com", AppID: os.Getenv("CYBERGRID_APP_ID"), OnError: func(err error) { log.Print(err) }})
	if err != nil {
		return err
	}
	var grantMu sync.Mutex
	var refresh string
	c.SetGrantCheckpoint(func(_ context.Context, _ *cybergrid.Client, t cybergrid.OAuthTokens) error {
		grantMu.Lock()
		refresh = t.RefreshToken
		grantMu.Unlock()
		return nil
	})
	c.SetAuthProvider(cybergrid.AuthProviderFunc(func(ctx context.Context, client *cybergrid.Client) (cybergrid.OAuthTokens, error) {
		grantMu.Lock()
		current := refresh
		grantMu.Unlock()
		return client.RefreshAccessToken(ctx, current)
	}))
	// This demo retains ownership until revocation completes, including errors
	// after the device grant or during resource exchange. Nothing enters the UI.
	defer func() {
		for {
			grantMu.Lock()
			current := refresh
			grantMu.Unlock()
			if current == "" {
				return
			}
			revokeCtx, stop := context.WithTimeout(context.Background(), 30*time.Second)
			e := c.RevokeRefreshToken(revokeCtx, current)
			stop()
			if e == nil {
				return
			}
			log.Print("Private grant revocation pending; retaining process ownership and retrying.")
			time.Sleep(5 * time.Second)
		}
	}()
	defer c.Close()
	auth, err := c.BeginDeviceAuthorization(ctx)
	if err != nil {
		return err
	}
	fmt.Printf("Open %s and enter %s\n", auth.VerificationURI, auth.UserCode)
	tokens, err := c.CompleteDeviceAuthorization(ctx, auth)
	if err != nil {
		return err
	}
	session, err := c.LoginWithAccessToken(ctx, tokens.AccessToken, "embedded-demo")
	tokens = cybergrid.OAuthTokens{}
	if err != nil {
		return err
	}
	if err = c.Expose(ctx, cybergrid.Port{ID: "api", Port: 8080, Type: "my-json-api", Transport: "tcp"}); err != nil {
		return err
	}
	devices, err := c.Devices(ctx)
	if err != nil {
		return err
	}
	var mappings []*cybergrid.Mapping
	defer func() {
		for _, m := range mappings {
			m.Close()
		}
	}()
	for _, d := range devices {
		if d.ID == session.DeviceID || !d.Online {
			continue
		}
		for _, p := range d.Ports {
			m, e := c.Map(ctx, d.ID, p.ID, "")
			if e != nil {
				log.Printf("cannot map %s/%s: %v", d.ID, p.ID, e)
				continue
			}
			mappings = append(mappings, m)
			fmt.Printf("%s/%s -> %s via %s\n", d.Name, p.Type, m.Addr, m.Transport)
		}
	}
	<-ctx.Done()
	return nil
}
