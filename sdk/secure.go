package cybergrid

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/hex"
	"errors"
	"io"
	"math/big"
	"net"
	"strings"
	"sync"
	"time"

	"github.com/xtaci/smux"
)

func newCertificate(id string) (tls.Certificate, string, error) {
	pub, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		return tls.Certificate{}, "", err
	}
	serial, err := rand.Int(rand.Reader, new(big.Int).Lsh(big.NewInt(1), 128))
	if err != nil {
		return tls.Certificate{}, "", err
	}
	now := time.Now()
	tpl := &x509.Certificate{SerialNumber: serial, Subject: pkix.Name{CommonName: id}, NotBefore: now.Add(-time.Minute), NotAfter: now.Add(24 * time.Hour), KeyUsage: x509.KeyUsageDigitalSignature, ExtKeyUsage: []x509.ExtKeyUsage{x509.ExtKeyUsageClientAuth, x509.ExtKeyUsageServerAuth}, BasicConstraintsValid: true}
	der, err := x509.CreateCertificate(rand.Reader, tpl, tpl, pub, key)
	if err != nil {
		return tls.Certificate{}, "", err
	}
	sum := sha256.Sum256(der)
	return tls.Certificate{Certificate: [][]byte{der}, PrivateKey: key}, hex.EncodeToString(sum[:]), nil
}
func certFingerprint(cert *x509.Certificate) string {
	s := sha256.Sum256(cert.Raw)
	return hex.EncodeToString(s[:])
}
func verifyPin(state tls.ConnectionState, id, pin string) error {
	if len(state.PeerCertificates) != 1 {
		return errors.New("expected exactly one peer certificate")
	}
	cert := state.PeerCertificates[0]
	if time.Now().Before(cert.NotBefore) || time.Now().After(cert.NotAfter) {
		return errors.New("peer certificate expired")
	}
	if cert.Subject.CommonName != id || !strings.EqualFold(certFingerprint(cert), pin) {
		return errors.New("peer certificate fingerprint does not match authenticated device")
	}
	return nil
}
func (c *Client) secureClient(ctx context.Context, raw net.Conn, d Device, transport string) (*Peer, error) {
	if d.CertificateFingerprint == nil {
		raw.Close()
		return nil, errors.New("peer has no certificate fingerprint")
	}
	c.mu.Lock()
	cert := c.cert
	c.mu.Unlock()
	conn := tls.Client(raw, &tls.Config{MinVersion: tls.VersionTLS13, Certificates: []tls.Certificate{cert}, InsecureSkipVerify: true, VerifyConnection: func(s tls.ConnectionState) error { return verifyPin(s, d.ID, *d.CertificateFingerprint) }})
	if err := conn.HandshakeContext(ctx); err != nil {
		raw.Close()
		return nil, err
	}
	config := smux.DefaultConfig()
	config.MaxReceiveBuffer = 4 << 20
	session, err := smux.Client(conn, config)
	if err != nil {
		conn.Close()
		return nil, err
	}
	return c.rememberPeer(d.ID, transport, session, raw), nil
}
func (c *Client) secureServer(ctx context.Context, raw net.Conn, transport string) (*Peer, error) {
	c.mu.Lock()
	cert := c.cert
	c.mu.Unlock()
	var peerID string
	conn := tls.Server(raw, &tls.Config{MinVersion: tls.VersionTLS13, Certificates: []tls.Certificate{cert}, ClientAuth: tls.RequireAnyClientCert, VerifyConnection: func(s tls.ConnectionState) error {
		if len(s.PeerCertificates) != 1 {
			return errors.New("missing client certificate")
		}
		id := s.PeerCertificates[0].Subject.CommonName
		d, err := c.device(ctx, id)
		if err != nil {
			return err
		}
		if d.CertificateFingerprint == nil {
			return errors.New("peer has no certificate")
		}
		if err = verifyPin(s, id, *d.CertificateFingerprint); err != nil {
			return err
		}
		peerID = id
		return nil
	}})
	if err := conn.HandshakeContext(ctx); err != nil {
		raw.Close()
		return nil, err
	}
	config := smux.DefaultConfig()
	config.MaxReceiveBuffer = 4 << 20
	session, err := smux.Server(conn, config)
	if err != nil {
		conn.Close()
		return nil, err
	}
	return c.rememberPeer(peerID, transport, session, raw), nil
}

// streamConn adapts detached DataChannels and WebSocket streams to the net.Conn
// interface required by TLS. The transport lifetime is separately canceled.
type streamConn struct {
	io.ReadWriteCloser
	local, remote net.Addr
	readMu        sync.Mutex
	pending       []byte
}

func (s *streamConn) Read(b []byte) (int, error) {
	s.readMu.Lock()
	defer s.readMu.Unlock()
	if len(b) == 0 {
		return 0, nil
	}
	if len(s.pending) == 0 {
		buf := make([]byte, 65536)
		n, err := s.ReadWriteCloser.Read(buf)
		if err != nil {
			return 0, err
		}
		s.pending = buf[:n]
	}
	n := copy(b, s.pending)
	s.pending = s.pending[n:]
	return n, nil
}
func (s *streamConn) Write(b []byte) (int, error) {
	n := 0
	for len(b) > 0 {
		chunk := len(b)
		if chunk > 16384 {
			chunk = 16384
		}
		written, err := s.ReadWriteCloser.Write(b[:chunk])
		n += written
		if err != nil {
			return n, err
		}
		if written != chunk {
			return n, io.ErrShortWrite
		}
		b = b[chunk:]
	}
	return n, nil
}
func (s *streamConn) LocalAddr() net.Addr  { return s.local }
func (s *streamConn) RemoteAddr() net.Addr { return s.remote }
func (s *streamConn) SetDeadline(t time.Time) error {
	if v, ok := s.ReadWriteCloser.(interface{ SetDeadline(time.Time) error }); ok {
		return v.SetDeadline(t)
	}
	return errors.New("transport deadlines unsupported")
}
func (s *streamConn) SetReadDeadline(t time.Time) error {
	if v, ok := s.ReadWriteCloser.(interface{ SetReadDeadline(time.Time) error }); ok {
		return v.SetReadDeadline(t)
	}
	return errors.New("transport read deadlines unsupported")
}
func (s *streamConn) SetWriteDeadline(t time.Time) error {
	if v, ok := s.ReadWriteCloser.(interface{ SetWriteDeadline(time.Time) error }); ok {
		return v.SetWriteDeadline(t)
	}
	return errors.New("transport write deadlines unsupported")
}

type namedAddr string

func (a namedAddr) Network() string { return "cybergrid" }
func (a namedAddr) String() string  { return string(a) }
