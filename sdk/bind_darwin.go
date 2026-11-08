//go:build darwin

package cybergrid

import (
	"golang.org/x/sys/unix"
	"net"
	"strings"
	"syscall"
)

func interfaceControl(name string) (func(string, string, syscall.RawConn) error, error) {
	iface, err := net.InterfaceByName(name)
	if err != nil {
		return nil, err
	}
	return func(network, address string, raw syscall.RawConn) error {
		var setErr error
		err := raw.Control(func(fd uintptr) {
			if strings.HasSuffix(network, "6") {
				setErr = unix.SetsockoptInt(int(fd), unix.IPPROTO_IPV6, unix.IPV6_BOUND_IF, iface.Index)
			} else {
				setErr = unix.SetsockoptInt(int(fd), unix.IPPROTO_IP, unix.IP_BOUND_IF, iface.Index)
			}
		})
		if err != nil {
			return err
		}
		return setErr
	}, nil
}
