//go:build !darwin

package cybergrid

import "syscall"

// Non-Darwin ICE binds each candidate socket to its discovered source address.
// Android application-level Network.bindSocket is a future embedding extension.
func interfaceControl(string) (func(string, string, syscall.RawConn) error, error) { return nil, nil }
