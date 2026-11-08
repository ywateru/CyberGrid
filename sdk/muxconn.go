package cybergrid

import (
	"encoding/binary"
	"errors"
	"github.com/xtaci/smux"
	"io"
	"net"
	"sync"
	"time"
)

// muxConn frames TCP FIN separately from smux stream close. This preserves
// response data after a caller performs TCP CloseWrite (HTTP/1.0 and similar).
type muxConn struct {
	stream            *smux.Stream
	local, remote     net.Addr
	writeMu           sync.Mutex
	readMu            sync.Mutex
	remaining         uint32
	readEOF, writeEOF bool
}

func newMuxConn(s *smux.Stream, local, remote string) *muxConn {
	return &muxConn{stream: s, local: namedAddr(local), remote: namedAddr(remote)}
}
func (m *muxConn) Read(b []byte) (int, error) {
	m.readMu.Lock()
	defer m.readMu.Unlock()
	if len(b) == 0 {
		return 0, nil
	}
	if m.readEOF {
		return 0, io.EOF
	}
	if m.remaining == 0 {
		var header [4]byte
		if _, err := io.ReadFull(m.stream, header[:]); err != nil {
			return 0, err
		}
		m.remaining = binary.BigEndian.Uint32(header[:])
		if m.remaining == 0 {
			m.readEOF = true
			return 0, io.EOF
		}
		if m.remaining > 32768 {
			return 0, errors.New("invalid stream frame length")
		}
	}
	if uint32(len(b)) > m.remaining {
		b = b[:m.remaining]
	}
	n, err := m.stream.Read(b)
	m.remaining -= uint32(n)
	return n, err
}
func (m *muxConn) Write(b []byte) (int, error) {
	m.writeMu.Lock()
	defer m.writeMu.Unlock()
	if m.writeEOF {
		return 0, io.ErrClosedPipe
	}
	written := 0
	for len(b) > 0 {
		n := len(b)
		if n > 32768 {
			n = 32768
		}
		var header [4]byte
		binary.BigEndian.PutUint32(header[:], uint32(n))
		if _, err := m.stream.Write(header[:]); err != nil {
			return written, err
		}
		sent, err := m.stream.Write(b[:n])
		written += sent
		if err != nil {
			return written, err
		}
		b = b[n:]
	}
	return written, nil
}
func (m *muxConn) CloseWrite() error {
	m.writeMu.Lock()
	defer m.writeMu.Unlock()
	if m.writeEOF {
		return nil
	}
	m.writeEOF = true
	_, err := m.stream.Write([]byte{0, 0, 0, 0})
	return err
}
func (m *muxConn) Close() error                       { return m.stream.Close() }
func (m *muxConn) LocalAddr() net.Addr                { return m.local }
func (m *muxConn) RemoteAddr() net.Addr               { return m.remote }
func (m *muxConn) SetDeadline(t time.Time) error      { return m.stream.SetDeadline(t) }
func (m *muxConn) SetReadDeadline(t time.Time) error  { return m.stream.SetReadDeadline(t) }
func (m *muxConn) SetWriteDeadline(t time.Time) error { return m.stream.SetWriteDeadline(t) }
