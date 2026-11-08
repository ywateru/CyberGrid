package cybergrid

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"sync"
	"time"

	"github.com/pion/transport/v5/stdnet"
	"github.com/pion/webrtc/v4"
)

type rtcAttempt struct {
	client            *Client
	id, remote        string
	pc                *webrtc.PeerConnection
	mu                sync.Mutex
	remoteSet         bool
	candidates        []webrtc.ICECandidateInit
	ready             chan net.Conn
	failed            chan error
	once              sync.Once
	closed            bool
	remoteIncarnation string
	signalReady       chan struct{}
	signalOnce        sync.Once
}

func randomID() string {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		panic(err)
	}
	return hex.EncodeToString(b[:])
}
func (a *rtcAttempt) close() {
	a.once.Do(func() {
		a.mu.Lock()
		a.closed = true
		a.mu.Unlock()
		a.signalOnce.Do(func() { close(a.signalReady) })
		_ = a.pc.Close()
		a.client.mu.Lock()
		delete(a.client.pending, a.id)
		a.client.mu.Unlock()
	})
}
func (c *Client) newAttempt(id, remote string, initiator bool) (*rtcAttempt, error) {
	settings := webrtc.SettingEngine{}
	settings.DetachDataChannels()
	settings.SetICETimeouts(5*time.Second, 10*time.Second, 2*time.Second)
	settings.SetNetworkTypes([]webrtc.NetworkType{webrtc.NetworkTypeUDP4, webrtc.NetworkTypeUDP6})
	if c.cfg.InterfaceName != "" {
		settings.SetInterfaceFilter(func(name string) bool { return name == c.cfg.InterfaceName })
	}
	settings.SetIncludeLoopbackCandidate(c.cfg.InterfaceName == "")
	if c.cfg.InterfaceName != "" {
		control, err := interfaceControl(c.cfg.InterfaceName)
		if err != nil {
			return nil, err
		}
		if control != nil {
			base, err := stdnet.NewNet()
			if err != nil {
				return nil, err
			}
			settings.SetNet(&boundNet{Net: base, control: control})
		}
	}
	api := webrtc.NewAPI(webrtc.WithSettingEngine(settings))
	servers := make([]webrtc.ICEServer, 0, len(c.cfg.STUNServers))
	for _, s := range c.cfg.STUNServers {
		servers = append(servers, webrtc.ICEServer{URLs: []string{s}})
	}
	pc, err := api.NewPeerConnection(webrtc.Configuration{ICEServers: servers})
	if err != nil {
		return nil, err
	}
	a := &rtcAttempt{client: c, id: id, remote: remote, pc: pc, ready: make(chan net.Conn, 1), failed: make(chan error, 1), signalReady: make(chan struct{})}
	c.mu.Lock()
	if c.ctx == nil || c.ctx.Err() != nil {
		c.mu.Unlock()
		pc.Close()
		return nil, errors.New("session is not active")
	}
	if len(c.pending) >= 32 {
		c.mu.Unlock()
		pc.Close()
		return nil, errors.New("too many pending connections")
	}
	if c.memory {
		generationCtx := c.ctx
		c.mu.Unlock()
		d, e := c.device(generationCtx, remote)
		c.mu.Lock()
		if e != nil {
			c.mu.Unlock()
			pc.Close()
			return nil, e
		}
		a.remoteIncarnation = d.Incarnation
		if c.ctx != generationCtx || generationCtx.Err() != nil {
			c.mu.Unlock()
			pc.Close()
			return nil, errors.New("generation closed")
		}
	}
	c.pending[id] = a
	ctx := c.ctx
	c.mu.Unlock()
	pc.OnICECandidate(func(candidate *webrtc.ICECandidate) {
		if candidate == nil {
			return
		}
		select {
		case <-a.signalReady:
		case <-ctx.Done():
			return
		}
		a.mu.Lock()
		closed := a.closed
		a.mu.Unlock()
		if closed {
			return
		}
		if err := c.sendSignal(ctx, remote, id, "candidate", candidate.ToJSON()); err != nil && ctx.Err() == nil {
			c.report(fmt.Errorf("ICE candidate: %w", err))
		}
	})
	pc.OnConnectionStateChange(func(state webrtc.PeerConnectionState) {
		if state == webrtc.PeerConnectionStateFailed || state == webrtc.PeerConnectionStateClosed {
			select {
			case a.failed <- fmt.Errorf("WebRTC %s", state.String()):
			default:
			}
		}
	})
	attach := func(dc *webrtc.DataChannel) {
		dc.OnOpen(func() {
			rw, err := dc.Detach()
			if err != nil {
				select {
				case a.failed <- err:
				default:
				}
				return
			}
			conn := &streamConn{ReadWriteCloser: rw, local: namedAddr(c.Session().DeviceID), remote: namedAddr(remote)}
			a.mu.Lock()
			closed := a.closed
			a.mu.Unlock()
			if closed {
				conn.Close()
				return
			}
			if initiator {
				select {
				case a.ready <- conn:
				default:
					conn.Close()
				}
			} else {
				go func() {
					handshake, cancel := context.WithTimeout(ctx, 10*time.Second)
					defer cancel()
					stop := context.AfterFunc(handshake, func() { conn.Close() })
					peer, err := c.secureServer(handshake, conn, "webrtc-direct")
					stop()
					if err != nil {
						conn.Close()
						a.close()
						c.report(fmt.Errorf("WebRTC peer authentication: %w", err))
						return
					}
					peer.setRTC(a)
				}()
			}
		})
	}
	if initiator {
		dc, err := pc.CreateDataChannel("cybergrid-v1", nil)
		if err != nil {
			a.close()
			return nil, err
		}
		attach(dc)
	} else {
		pc.OnDataChannel(func(dc *webrtc.DataChannel) {
			if dc.Label() != "cybergrid-v1" {
				dc.Close()
				return
			}
			attach(dc)
		})
	}
	// Unanswered and malicious offers cannot retain sockets forever.
	go func() {
		select {
		case <-ctx.Done():
			a.close()
		case <-time.After(2*c.cfg.DirectTimeout + 15*time.Second):
			c.mu.Lock()
			pending := c.pending[id] == a
			hasPeer := false
			for _, p := range c.peers {
				if p.hasRTC(a) && !p.session.IsClosed() {
					hasPeer = true
				}
			}
			c.mu.Unlock()
			if pending && !hasPeer {
				a.close()
			}
		}
	}()
	return a, nil
}
func (a *rtcAttempt) setRemote(desc webrtc.SessionDescription) error {
	if err := a.pc.SetRemoteDescription(desc); err != nil {
		return err
	}
	a.mu.Lock()
	a.remoteSet = true
	cs := a.candidates
	a.candidates = nil
	a.mu.Unlock()
	for _, v := range cs {
		if err := a.pc.AddICECandidate(v); err != nil {
			return err
		}
	}
	return nil
}
func (c *Client) handleSignal(s Signal) error {
	if s.Kind != "offer" && s.Kind != "answer" && s.Kind != "candidate" && s.Kind != "close" {
		return errors.New("unknown signal kind")
	}
	if len(s.Payload) > 65536 || len(s.ConnectionID) > 128 || len(s.FromDeviceID) > 128 || s.ConnectionID == "" || s.FromDeviceID == "" {
		return errors.New("invalid signaling response")
	}
	c.mu.Lock()
	a := c.pending[s.ConnectionID]
	ctx := c.ctx
	c.mu.Unlock()
	if s.Kind == "offer" {
		if a != nil {
			return nil
		}
		if _, err := c.device(ctx, s.FromDeviceID); err != nil {
			return err
		}
		var desc webrtc.SessionDescription
		if err := json.Unmarshal([]byte(s.Payload), &desc); err != nil {
			return err
		}
		if desc.Type != webrtc.SDPTypeOffer {
			return errors.New("expected SDP offer")
		}
		var err error
		a, err = c.newAttempt(s.ConnectionID, s.FromDeviceID, false)
		if err != nil {
			return err
		}
		if err = a.setRemote(desc); err != nil {
			a.close()
			return err
		}
		answer, err := a.pc.CreateAnswer(nil)
		if err != nil {
			a.close()
			return err
		}
		if err = a.pc.SetLocalDescription(answer); err != nil {
			a.close()
			return err
		}
		if err = c.sendSignal(ctx, s.FromDeviceID, s.ConnectionID, "answer", answer); err != nil {
			a.close()
			return err
		}
		a.signalOnce.Do(func() { close(a.signalReady) })
		return nil
	}
	if a == nil {
		return nil
	}
	if a.remote != s.FromDeviceID {
		return errors.New("signaling sender mismatch")
	}
	switch s.Kind {
	case "answer":
		var d webrtc.SessionDescription
		if err := json.Unmarshal([]byte(s.Payload), &d); err != nil {
			return err
		}
		if d.Type != webrtc.SDPTypeAnswer {
			return errors.New("expected SDP answer")
		}
		return a.setRemote(d)
	case "candidate":
		var candidate webrtc.ICECandidateInit
		if err := json.Unmarshal([]byte(s.Payload), &candidate); err != nil {
			return err
		}
		a.mu.Lock()
		if !a.remoteSet {
			if len(a.candidates) >= 128 {
				a.mu.Unlock()
				return errors.New("candidate limit exceeded")
			}
			a.candidates = append(a.candidates, candidate)
			a.mu.Unlock()
			return nil
		}
		a.mu.Unlock()
		return a.pc.AddICECandidate(candidate)
	case "close":
		a.close()
		return nil
	default:
		return errors.New("unknown signal kind")
	}
}
func (c *Client) dialDirect(ctx context.Context, d Device) (*Peer, error) {
	attemptCtx, cancel := context.WithTimeout(ctx, c.cfg.DirectTimeout)
	defer cancel()
	a, err := c.newAttempt(randomID(), d.ID, true)
	if err != nil {
		return nil, err
	}
	offer, err := a.pc.CreateOffer(nil)
	if err != nil {
		a.close()
		return nil, err
	}
	if err = a.pc.SetLocalDescription(offer); err != nil {
		a.close()
		return nil, err
	}
	if err = c.sendSignal(attemptCtx, d.ID, a.id, "offer", offer); err != nil {
		a.close()
		return nil, err
	}
	a.signalOnce.Do(func() { close(a.signalReady) })
	select {
	case raw := <-a.ready:
		stop := context.AfterFunc(attemptCtx, func() { raw.Close() })
		p, err := c.secureClient(attemptCtx, raw, d, "webrtc-direct")
		stop()
		if err != nil {
			raw.Close()
			a.close()
			return nil, err
		}
		p.setRTC(a)
		return p, nil
	case err := <-a.failed:
		a.close()
		return nil, err
	case <-attemptCtx.Done():
		a.close()
		return nil, attemptCtx.Err()
	}
}
