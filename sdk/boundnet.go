package cybergrid

import (
	"context"
	"github.com/pion/transport/v5"
	"net"
	"syscall"
)

type boundNet struct {
	transport.Net
	control func(string, string, syscall.RawConn) error
}

func (n *boundNet) ListenPacket(network, address string) (net.PacketConn, error) {
	cfg := net.ListenConfig{Control: n.control}
	return cfg.ListenPacket(context.Background(), network, address)
}
func (n *boundNet) ListenUDP(network string, address *net.UDPAddr) (transport.UDPConn, error) {
	value := ""
	if address != nil {
		value = address.String()
	}
	conn, err := n.ListenPacket(network, value)
	if err != nil {
		return nil, err
	}
	return conn.(*net.UDPConn), nil
}
