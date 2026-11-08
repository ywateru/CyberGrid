package cybergrid

import (
	"context"
	"encoding/binary"
	"errors"
	"fmt"
	"github.com/xtaci/smux"
	"io"
	"net"
	"strconv"
	"sync"
	"time"
)

type Peer struct {
	DeviceID    string
	Transport   string
	client      *Client
	session     *smux.Session
	raw         net.Conn
	rtc         *rtcAttempt
	once        sync.Once
	mu          sync.Mutex
	stopContext func() bool
}

func (c *Client) rememberPeer(id, transport string, session *smux.Session, raw net.Conn) *Peer {
	p := &Peer{DeviceID: id, Transport: transport, client: c, session: session, raw: raw}
	c.mu.Lock()
	c.peers[randomID()] = p
	ctx := c.ctx
	c.mu.Unlock()
	stop := context.AfterFunc(ctx, func() { p.Close() })
	p.mu.Lock()
	p.stopContext = stop
	closed := p.session.IsClosed()
	p.mu.Unlock()
	if closed {
		stop()
	}
	c.event("multiplexer-ready", transport, id)
	go p.serve(ctx)
	return p
}
func (p *Peer) Close() error {
	var err error
	p.once.Do(func() {
		// Close the raw transport first so TLS close_notify cannot wait on a
		// stalled peer or a blocked WebSocket writer during cancellation.
		err = p.raw.Close()
		_ = p.session.Close()
		if errors.Is(err, net.ErrClosed) || errors.Is(err, io.ErrClosedPipe) {
			err = nil
		}
		p.client.mu.Lock()
		for id, v := range p.client.peers {
			if v == p {
				delete(p.client.peers, id)
			}
		}
		p.client.mu.Unlock()
		p.mu.Lock()
		rtc := p.rtc
		stop := p.stopContext
		p.mu.Unlock()
		if stop != nil {
			stop()
		}
		if rtc != nil {
			rtc.close()
		}
	})
	return err
}
func (c *Client) Connect(ctx context.Context, deviceID string) (*Peer, error) {
	c.mu.Lock()
	blocked := c.memory && !c.ready
	c.mu.Unlock()
	if blocked {
		return nil, &APIError{Status: 503, Code: "COORDINATOR_RECOVERING", Message: "coordinator recovery is not ready"}
	}
	d, err := c.device(ctx, deviceID)
	if err != nil {
		return nil, err
	}
	if d.ID == c.Session().DeviceID {
		return nil, errors.New("cannot connect to self")
	}
	p, err := c.dialDirect(ctx, d)
	if err == nil {
		return p, nil
	}
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	directErr := err
	fallbackCtx, cancel := context.WithTimeout(ctx, c.cfg.FallbackTimeout)
	defer cancel()
	// The provider can come online during the direct attempt. Refresh its
	// authenticated metadata, never reuse a stale URL or certificate pin.
	for {
		d, err = c.device(fallbackCtx, deviceID)
		if err != nil {
			return nil, fmt.Errorf("direct: %v; refreshing peer for fallback: %w", directErr, err)
		}
		if d.TunnelURL != nil && *d.TunnelURL != "" {
			break
		}
		select {
		case <-fallbackCtx.Done():
			return nil, fmt.Errorf("direct: %v; remote Quick Tunnel did not become ready: %w", directErr, fallbackCtx.Err())
		case <-time.After(500 * time.Millisecond):
		}
	}
	p, err = c.dialTunnel(fallbackCtx, d)
	if err != nil {
		return nil, fmt.Errorf("direct: %v; tunnel fallback: %w", directErr, err)
	}
	return p, nil
}
func (p *Peer) Dial(ctx context.Context, portID string) (net.Conn, error) {
	if len(portID) == 0 || len(portID) > 64 {
		return nil, errors.New("invalid port ID")
	}
	stream, err := p.session.OpenStream()
	if err != nil {
		return nil, err
	}
	deadline := time.Now().Add(10 * time.Second)
	if d, ok := ctx.Deadline(); ok && d.Before(deadline) {
		deadline = d
	}
	_ = stream.SetDeadline(deadline)
	stop := context.AfterFunc(ctx, func() { stream.Close() })
	defer stop()
	header := make([]byte, 2+len(portID))
	binary.BigEndian.PutUint16(header[:2], uint16(len(portID)))
	copy(header[2:], portID)
	if _, err = stream.Write(header); err != nil {
		stream.Close()
		return nil, err
	}
	var ack [1]byte
	if _, err = io.ReadFull(stream, ack[:]); err != nil {
		stream.Close()
		return nil, err
	}
	if ack[0] != 0 {
		stream.Close()
		return nil, errors.New("remote port is not exposed or could not be opened")
	}
	_ = stream.SetDeadline(time.Time{})
	p.client.event("port-open", p.Transport, p.DeviceID)
	return newMuxConn(stream, p.client.Session().DeviceID, p.DeviceID+"/"+portID), nil
}
func (p *Peer) serve(ctx context.Context) {
	defer p.Close()
	for {
		stream, err := p.session.AcceptStream()
		if err != nil {
			return
		}
		go func() {
			defer stream.Close()
			_ = stream.SetDeadline(time.Now().Add(10 * time.Second))
			var size [2]byte
			if _, err := io.ReadFull(stream, size[:]); err != nil {
				return
			}
			n := int(binary.BigEndian.Uint16(size[:]))
			if n < 1 || n > 64 {
				return
			}
			b := make([]byte, n)
			if _, err := io.ReadFull(stream, b); err != nil {
				return
			}
			p.client.mu.Lock()
			port, ok := p.client.ports[string(b)]
			p.client.mu.Unlock()
			if !ok {
				_, _ = stream.Write([]byte{1})
				return
			}
			dialer := net.Dialer{Timeout: 5 * time.Second}
			target, err := dialer.DialContext(ctx, "tcp", net.JoinHostPort("127.0.0.1", strconv.Itoa(port.Port)))
			if err != nil {
				_, _ = stream.Write([]byte{1})
				return
			}
			defer target.Close()
			if _, err = stream.Write([]byte{0}); err != nil {
				return
			}
			_ = stream.SetDeadline(time.Time{})
			bridge(ctx, newMuxConn(stream, p.client.Session().DeviceID, p.DeviceID), target)
		}()
	}
}
func bridge(ctx context.Context, a, b io.ReadWriteCloser) {
	stop := context.AfterFunc(ctx, func() { a.Close(); b.Close() })
	defer stop()
	var wg sync.WaitGroup
	wg.Add(2)
	pump := func(dst, src io.ReadWriteCloser) {
		defer wg.Done()
		_, err := io.Copy(dst, src)
		if err != nil {
			a.Close()
			b.Close()
			return
		}
		if half, ok := dst.(interface{ CloseWrite() error }); ok {
			_ = half.CloseWrite()
		} else {
			dst.Close()
		}
	}
	go pump(a, b)
	go pump(b, a)
	wg.Wait()
	a.Close()
	b.Close()
}
func (p *Peer) setRTC(a *rtcAttempt) {
	p.mu.Lock()
	p.rtc = a
	closed := p.session.IsClosed()
	p.mu.Unlock()
	if closed {
		a.close()
	}
}
func (p *Peer) hasRTC(a *rtcAttempt) bool { p.mu.Lock(); defer p.mu.Unlock(); return p.rtc == a }

type Mapping struct {
	Addr      net.Addr
	Transport string
	Peer      *Peer
	listener  net.Listener
	cancel    context.CancelFunc
	once      sync.Once
	mu        sync.Mutex
	wg        sync.WaitGroup
	client    *Client
	tracked   map[*Peer]struct{}
}

func (m *Mapping) Close() error {
	var err error
	m.once.Do(func() {
		m.cancel()
		err = m.listener.Close()
		if m.Peer != nil {
			m.Peer.Close()
		}
		m.mu.Lock()
		for p := range m.tracked {
			p.Close()
		}
		m.mu.Unlock()
		m.wg.Wait()
		if m.client != nil {
			m.client.mu.Lock()
			delete(m.client.mappings, m)
			m.client.mu.Unlock()
		}
	})
	return err
}
func (c *Client) Map(ctx context.Context, deviceID, portID, localAddress string) (*Mapping, error) {
	if localAddress == "" {
		localAddress = "127.0.0.1:0"
	}
	host, _, err := net.SplitHostPort(localAddress)
	if err != nil {
		return nil, err
	}
	ip := net.ParseIP(host)
	if ip == nil || !ip.IsLoopback() {
		return nil, errors.New("local mapping must bind a loopback IP")
	}
	c.mu.Lock()
	memory := c.memory
	c.mu.Unlock()
	if memory {
		return c.mapMemory(ctx, deviceID, portID, localAddress)
	}
	peer, err := c.Connect(ctx, deviceID)
	if err != nil {
		return nil, err
	}
	probe, err := peer.Dial(ctx, portID)
	if err != nil {
		peer.Close()
		return nil, err
	}
	probe.Close()
	ln, err := net.Listen("tcp", localAddress)
	if err != nil {
		peer.Close()
		return nil, err
	}
	mappingCtx, cancel := context.WithCancel(ctx)
	m := &Mapping{Addr: ln.Addr(), Transport: peer.Transport, Peer: peer, listener: ln, cancel: cancel}
	stop := context.AfterFunc(c.ctx, cancel)
	m.wg.Add(1)
	go func() {
		defer m.wg.Done()
		defer stop()
		go func() {
			select {
			case <-peer.session.CloseChan():
				cancel()
			case <-mappingCtx.Done():
			}
		}()
		stopListener := context.AfterFunc(mappingCtx, func() { ln.Close(); peer.Close() })
		defer stopListener()
		for {
			local, err := ln.Accept()
			if err != nil {
				return
			}
			m.wg.Add(1)
			go func() {
				defer m.wg.Done()
				defer local.Close()
				remote, err := peer.Dial(mappingCtx, portID)
				if err != nil {
					c.report(err)
					return
				}
				defer remote.Close()
				bridge(mappingCtx, local, remote)
			}()
		}
	}()
	return m, nil
}

// SelectedPath reports the actual nominated ICE pair for direct connections.
// Tunnel connections have no ICE pair. Candidate strings may contain IPs.
func (p *Peer) SelectedPath() map[string]string {
	path := map[string]string{"transport": p.Transport}
	p.mu.Lock()
	rtc := p.rtc
	p.mu.Unlock()
	if rtc != nil {
		if pair, err := rtc.pc.SCTP().Transport().ICETransport().GetSelectedCandidatePair(); err == nil && pair != nil {
			path["localCandidate"] = pair.Local.String()
			path["remoteCandidate"] = pair.Remote.String()
		}
	}
	return path
}
