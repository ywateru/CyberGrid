package cybergrid

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"errors"
	"fmt"
	"math/big"
	"net"
	"net/url"
	"strings"
	"time"
)

var errRecoveryIdentity = errors.New("recovery identity subject changed")

const maxSafeRevision int64 = 1<<53 - 1

func uuidID() string {
	var b [16]byte
	if _, e := rand.Read(b[:]); e != nil {
		panic(e)
	}
	b[6] = b[6]&15 | 64
	b[8] = b[8]&63 | 128
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[:4], b[4:6], b[6:8], b[8:10], b[10:])
}
func (c *Client) SetAuthProvider(provider AuthProvider) {
	c.mu.Lock()
	c.authProvider = provider
	c.mu.Unlock()
}
func (c *Client) coordinationEvent(phase, reason string) {
	c.mu.Lock()
	generation, epoch := c.generation, c.session.CoordinatorEpoch
	c.mu.Unlock()
	if c.cfg.OnTransportEvent != nil {
		c.cfg.OnTransportEvent(TransportEvent{Phase: phase, Generation: generation, Epoch: epoch, Reason: reason})
	}
}
func (c *Client) loginMemory(ctx context.Context, token, name string, a AuthConfig) (Session, error) {
	c.recoverMu.Lock()
	defer c.recoverMu.Unlock()
	c.mu.Lock()
	if c.session.SessionKey != "" || c.closing {
		c.mu.Unlock()
		return Session{}, errors.New("client already logged in or closed; create a new client")
	}
	if c.registrationKey == "" {
		b := make([]byte, 32)
		if _, e := rand.Read(b); e != nil {
			c.mu.Unlock()
			return Session{}, e
		}
		c.registrationKey = base64.RawURLEncoding.EncodeToString(b)
	}
	c.memory = true
	c.deviceName = name
	c.lifetime, c.lifetimeCancel = context.WithCancel(context.Background())
	c.mu.Unlock()
	c.mu.Lock()
	if c.registrationRequestID == "" {
		c.registrationRequestID = uuidID()
	}
	requestID := c.registrationRequestID
	c.mu.Unlock()
	var s Session
	var err error
	for attempt := 0; ; attempt++ {
		s, err = c.registerMemory(ctx, token, a, requestID)
		if err == nil {
			err = c.activateMemory(ctx, s)
			if err == nil {
				break
			}
			c.shutdown()
		}
		if ctx.Err() != nil || attempt >= 5 {
			return Session{}, err
		}
		var api *APIError
		if errors.As(err, &api) && api.Code == "COORDINATOR_RESET" {
			a, err = c.AuthConfig(ctx)
			if err != nil {
				return Session{}, err
			}
			requestID = uuidID()
			c.mu.Lock()
			c.registrationRequestID = requestID
			c.mu.Unlock()
			continue
		}
		if !transient(err) {
			return Session{}, err
		}
		select {
		case <-ctx.Done():
			return Session{}, ctx.Err()
		case <-time.After(retryDelay(attempt, err)):
		}
	}
	c.mu.Lock()
	c.initialUser = s.User.ID
	if c.closing {
		c.mu.Unlock()
		return Session{}, context.Canceled
	}
	c.wg.Add(1)
	c.mu.Unlock()
	go c.run()
	return s, nil
}
func (c *Client) registerMemory(ctx context.Context, token string, a AuthConfig, requestID string) (Session, error) {
	c.mu.Lock()
	if c.registrationRevision == 0 {
		c.registrationRevision = 1
	}
	revision, key, name := c.registrationRevision, c.registrationKey, c.deviceName
	c.mu.Unlock()
	var s Session
	err := c.request(ctx, "POST", "/auth/exchange", map[string]any{"accessToken": token, "appId": c.cfg.AppID, "deviceName": name, "coordinationProtocol": "memory-v1", "coordinatorEpoch": a.Coordination.Epoch, "registrationKey": key, "requestId": requestID, "registrationRevision": revision}, &s, false)
	if err != nil {
		return Session{}, err
	}
	if s.Purpose != "device" || s.AppID != c.cfg.AppID || s.CoordinatorEpoch != a.Coordination.Epoch || s.Incarnation == "" || s.SessionKey == "" || !s.ExpiresAt.After(time.Now()) || !strings.HasPrefix(s.DeviceID, "d_") || len(s.DeviceID) != 50 || s.User.ID == "" {
		return Session{}, errors.New("invalid memory-v1 registration response")
	}
	return s, nil
}
func (c *Client) activateMemory(ctx context.Context, s Session) error {
	c.mu.Lock()
	same := c.session.Incarnation == s.Incarnation && c.session.CoordinatorEpoch == s.CoordinatorEpoch && c.session.DeviceID == s.DeviceID && c.fingerprint != ""
	cert, fp := c.cert, c.fingerprint
	c.mu.Unlock()
	var err error
	if !same {
		cert, fp, err = newCertificate(s.DeviceID)
		if err != nil {
			return err
		}
	}
	c.publishMu.Lock()
	c.mu.Lock()
	if c.closing || c.lifetime.Err() != nil {
		c.mu.Unlock()
		c.publishMu.Unlock()
		return context.Canceled
	}
	if c.initialUser != "" && s.User.ID != c.initialUser {
		c.mu.Unlock()
		c.publishMu.Unlock()
		return errRecoveryIdentity
	}
	c.session = s
	c.cert = cert
	c.fingerprint = fp
	c.ctx, c.cancel = context.WithCancel(c.lifetime)
	c.cursor = 0
	if !same {
		c.publishRevision = 0
	}
	c.generation++
	c.ready = false
	c.tunnelURL = ""
	c.tunnel = nil
	c.peers = map[string]*Peer{}
	c.pending = map[string]*rtcAttempt{}
	generationCtx := c.ctx
	c.mu.Unlock()
	c.publishMu.Unlock()
	if err = c.startOrigin(); err != nil {
		return err
	}
	if err = c.heartbeat(generationCtx); err != nil {
		return err
	}
	c.mu.Lock()
	if c.closing || generationCtx.Err() != nil {
		c.mu.Unlock()
		return context.Canceled
	}
	c.ready = true
	startProvider := !c.cfg.DisableTunnel
	if startProvider {
		c.wg.Add(1)
	}
	c.mu.Unlock()
	c.coordinationEvent("coordination-ready", "")
	if startProvider {
		go c.startTunnelGeneration(generationCtx)
	}
	return nil
}
func (c *Client) recoveryReason(err error) bool {
	var a *APIError
	return errors.As(err, &a) && (a.Code == "COORDINATOR_RESET" || a.Code == "SESSION_EXPIRED")
}
func transient(err error) bool {
	var a *APIError
	if errors.As(err, &a) {
		return a.Status == 429 || a.Status >= 500
	}
	var o *OAuthError
	if errors.As(err, &o) {
		return o.Status == 429 || o.Status >= 500
	}
	return true
}
func retryDelay(attempt int, err error) time.Duration {
	var a *APIError
	if errors.As(err, &a) && a.RetryAfter > 0 {
		return a.RetryAfter
	}
	if attempt > 5 {
		attempt = 5
	}
	base := time.Second * time.Duration(1<<attempt)
	j, _ := rand.Int(rand.Reader, big.NewInt(int64(base/2)+1))
	return base + jDuration(j)
}
func jDuration(v *big.Int) time.Duration {
	if v == nil {
		return 0
	}
	return time.Duration(v.Int64())
}
func (c *Client) recoverMemory(reason string) bool {
	c.recoverMu.Lock()
	defer c.recoverMu.Unlock()
	c.mu.Lock()
	if c.closing {
		c.mu.Unlock()
		return false
	}
	c.ready = false
	provider := c.authProvider
	life := c.lifetime
	if c.registrationRevision >= maxSafeRevision {
		c.mu.Unlock()
		c.shutdown()
		c.coordinationEvent("requires-login", "registration-revision-exhausted")
		return false
	}
	c.registrationRevision++
	c.mu.Unlock()
	c.coordinationEvent("recovering", reason)
	c.shutdown()
	c.coordinationEvent("mapping-disconnected", reason)
	if provider == nil {
		c.coordinationEvent("requires-login", "no-auth-provider")
		return false
	}
	requestID := uuidID()
	var tokens OAuthTokens
	var epoch string
	attempt := 0
	for {
		if life.Err() != nil {
			return false
		}
		a, err := c.AuthConfig(life)
		if err == nil && a.Coordination.Protocol != "memory-v1" {
			err = errors.New("memory-v1 negotiation required")
			c.report(err)
			c.coordinationEvent("requires-login", "protocol-unavailable")
			return false
		}
		if err == nil {
			if epoch != "" && epoch != a.Coordination.Epoch {
				requestID = uuidID()
			}
			epoch = a.Coordination.Epoch
			if tokens.AccessToken == "" || !tokens.ExpiresAt.After(time.Now().Add(30*time.Second)) {
				tokens, err = provider.AccessToken(life, c)
			}
			if err == nil {
				var s Session
				s, err = c.registerMemory(life, tokens.AccessToken, a, requestID)
				if err == nil {
					err = c.activateMemory(life, s)
					if err == nil {
						return true
					}
					c.shutdown()
				}
			}
		}
		if life.Err() != nil {
			return false
		}
		if errors.Is(err, errRecoveryIdentity) || errors.Is(err, errAuthBinding) {
			c.report(err)
			c.coordinationEvent("requires-login", "identity-changed")
			return false
		}
		var api *APIError
		if errors.As(err, &api) {
			if api.Code == "COORDINATOR_RESET" {
				requestID = uuidID()
				continue
			}
			if api.Code == "REGISTRATION_ATTEMPT_EXPIRED" {
				c.mu.Lock()
				if c.registrationRevision >= maxSafeRevision {
					c.mu.Unlock()
					c.coordinationEvent("requires-login", "registration-revision-exhausted")
					return false
				}
				c.registrationRevision++
				c.mu.Unlock()
				requestID = uuidID()
				continue
			}
			if api.Code == "SESSION_SUPERSEDED" || api.Status == 401 || api.Status == 403 {
				c.report(err)
				c.coordinationEvent("requires-login", api.Code)
				return false
			}
		}
		var oauth *OAuthError
		if errors.As(err, &oauth) && oauth.Status < 500 && oauth.Status != 429 {
			c.report(err)
			c.coordinationEvent("requires-login", oauth.Code)
			return false
		}
		c.report(err)
		delay := retryDelay(attempt, err)
		attempt++
		select {
		case <-life.Done():
			return false
		case <-time.After(delay):
		}
	}
}
func (c *Client) runMemory() {
	lastBeat := time.Now()
	for {
		c.mu.Lock()
		ctx, life := c.ctx, c.lifetime
		expiry, last, cursor := c.session.ExpiresAt, c.lastHeartbeat, c.cursor
		c.mu.Unlock()
		if life.Err() != nil {
			return
		}
		if !time.Now().Before(expiry) || time.Since(last) > 65*time.Second {
			if !c.recoverMemory("lease-or-session-expired") {
				return
			}
			lastBeat = time.Now()
			continue
		}
		if time.Since(lastBeat) >= 20*time.Second {
			err := c.heartbeat(ctx)
			if err == nil {
				lastBeat = time.Now()
			} else {
				c.report(err)
				if c.recoveryReason(err) {
					if !c.recoverMemory("coordinator-reset") {
						return
					}
					lastBeat = time.Now()
					continue
				}
				if c.fatal(err) {
					c.shutdown()
					c.coordinationEvent("requires-login", "identity-rejected")
					return
				}
			}
		}
		var out struct {
			Signals []Signal `json:"signals"`
			Cursor  int64    `json:"cursor"`
			Epoch   string   `json:"coordinatorEpoch"`
		}
		err := c.request(ctx, "GET", fmt.Sprintf("/signals?after=%d&wait=15", cursor), nil, &out, true)
		if err != nil {
			if life.Err() != nil {
				return
			}
			if c.recoveryReason(err) {
				if !c.recoverMemory("coordinator-reset") {
					return
				}
				lastBeat = time.Now()
				continue
			}
			if c.fatal(err) {
				c.shutdown()
				c.coordinationEvent("requires-login", "identity-rejected")
				return
			}
			c.report(err)
			select {
			case <-life.Done():
				return
			case <-time.After(retryDelay(0, err)):
			}
			continue
		}
		c.mu.Lock()
		epoch, inc := c.session.CoordinatorEpoch, c.session.Incarnation
		c.mu.Unlock()
		if out.Epoch != epoch {
			if !c.recoverMemory("coordinator-reset") {
				return
			}
			continue
		}
		if out.Cursor < cursor {
			c.report(errors.New("memory signaling cursor moved backwards"))
			continue
		}
		processed := true
		for _, s := range out.Signals {
			if s.ID <= cursor {
				continue
			}
			if s.CoordinatorEpoch != epoch || s.ToIncarnation != inc {
				cursor = s.ID
				continue
			}
			remote, e := c.device(ctx, s.FromDeviceID)
			if e == nil && remote.Incarnation == s.FromIncarnation {
				e = c.handleSignal(s)
			}
			if e != nil {
				c.report(e)
				var api *APIError
				if errors.As(e, &api) && (api.Status >= 500 || api.Status == 429) {
					processed = false
					break
				}
				var transportError *url.Error
				if errors.As(e, &transportError) {
					processed = false
					break
				}
			}
			cursor = s.ID
		}
		if processed {
			cursor = out.Cursor
		}
		c.mu.Lock()
		if c.ctx == ctx {
			c.cursor = cursor
		}
		c.mu.Unlock()
	}
}
func (c *Client) sendMemorySignal(ctx context.Context, id, connection, kind string, p any) error {
	body := map[string]string{"toDeviceId": id, "connectionId": connection, "kind": kind, "payload": payload(p)}
	c.mu.Lock()
	memory := c.memory
	attempt := c.pending[connection]
	c.mu.Unlock()
	if !memory {
		return c.request(ctx, "POST", "/signals", body, nil, true)
	}
	d, err := c.device(ctx, id)
	if err != nil {
		return err
	}
	if attempt != nil && attempt.remoteIncarnation != "" && attempt.remoteIncarnation != d.Incarnation {
		return errors.New("remote incarnation changed")
	}
	body["toIncarnation"] = d.Incarnation
	body["clientMessageId"] = uuidID()
	deadline := time.Now().Add(10 * time.Second)
	for n := 0; ; n++ {
		err = c.request(ctx, "POST", "/signals", body, nil, true)
		if err == nil || !transient(err) || time.Now().After(deadline) {
			return err
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(time.Duration(n+1) * 200 * time.Millisecond):
		}
	}
}

func (c *Client) mapMemory(ctx context.Context, device, port, address string) (*Mapping, error) {
	p, err := c.Connect(ctx, device)
	if err != nil {
		return nil, err
	}
	probe, err := p.Dial(ctx, port)
	if err != nil {
		p.Close()
		return nil, err
	}
	probe.Close()
	ln, err := net.Listen("tcp", address)
	if err != nil {
		p.Close()
		return nil, err
	}
	mappingCtx, cancel := context.WithCancel(ctx)
	m := &Mapping{Addr: ln.Addr(), Transport: p.Transport, Peer: p, listener: ln, cancel: cancel, client: c, tracked: map[*Peer]struct{}{}}
	p.Close()
	c.mu.Lock()
	if c.closing {
		c.mu.Unlock()
		ln.Close()
		cancel()
		return nil, context.Canceled
	}
	c.mappings[m] = struct{}{}
	life := c.lifetime
	c.mu.Unlock()
	stop := context.AfterFunc(life, cancel)
	m.wg.Add(1)
	go func() {
		defer m.wg.Done()
		defer stop()
		stopListener := context.AfterFunc(mappingCtx, func() { ln.Close() })
		defer stopListener()
		for {
			local, e := ln.Accept()
			if e != nil {
				return
			}
			m.wg.Add(1)
			go func() {
				defer m.wg.Done()
				defer local.Close()
				c.mu.Lock()
				ready, genCtx := c.ready, c.ctx
				c.mu.Unlock()
				if !ready {
					c.coordinationEvent("mapping-disconnected", "recovering")
					return
				}
				connCtx, done := context.WithCancel(mappingCtx)
				stopGen := context.AfterFunc(genCtx, done)
				defer done()
				defer stopGen()
				current, e := c.Connect(connCtx, device)
				if e != nil {
					c.report(e)
					c.coordinationEvent("mapping-disconnected", "connect-failed")
					return
				}
				m.mu.Lock()
				if mappingCtx.Err() != nil {
					m.mu.Unlock()
					current.Close()
					return
				}
				m.tracked[current] = struct{}{}
				m.mu.Unlock()
				defer func() { current.Close(); m.mu.Lock(); delete(m.tracked, current); m.mu.Unlock() }()
				remote, e := current.Dial(connCtx, port)
				if e != nil {
					c.report(e)
					return
				}
				defer remote.Close()
				c.coordinationEvent("mapping-ready", "")
				bridge(connCtx, local, remote)
			}()
		}
	}()
	return m, nil
}
