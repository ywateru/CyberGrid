// Package cybergrid provides account-scoped device discovery and authenticated
// TCP mappings over direct WebRTC with Cloudflare Quick Tunnel fallback.
package cybergrid

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

type Port struct {
	ID        string `json:"id"`
	Port      int    `json:"port"`
	Type      string `json:"type"`
	Transport string `json:"transport"`
}
type Device struct {
	ID                     string    `json:"id"`
	Name                   string    `json:"name"`
	Online                 bool      `json:"online"`
	LastSeen               time.Time `json:"lastSeen"`
	Ports                  []Port    `json:"ports"`
	TunnelURL              *string   `json:"tunnelUrl"`
	CertificateFingerprint *string   `json:"certificateFingerprint"`
	Incarnation            string    `json:"incarnation,omitempty"`
	PublishRevision        int64     `json:"publishRevision,omitempty"`
}
type User struct {
	ID    string `json:"id"`
	Email string `json:"email,omitempty"`
}
type Session struct {
	Purpose          string    `json:"purpose"`
	AppID            string    `json:"appId"`
	SessionKey       string    `json:"sessionKey"`
	ExpiresAt        time.Time `json:"expiresAt"`
	DeviceID         string    `json:"deviceId"`
	User             User      `json:"user"`
	CoordinatorEpoch string    `json:"coordinatorEpoch,omitempty"`
	Incarnation      string    `json:"incarnation,omitempty"`
}

// Application is a read-only Logto Native Device Flow directory entry.
// Listing it does not imply that the current user has authorized that application.
type Application struct {
	ID           string `json:"id"`
	Name         string `json:"name"`
	Type         string `json:"type"`
	IsDeviceFlow bool   `json:"isDeviceFlow"`
}
type Signal struct {
	ID               int64  `json:"id"`
	FromDeviceID     string `json:"fromDeviceId"`
	ConnectionID     string `json:"connectionId"`
	Kind             string `json:"kind"`
	Payload          string `json:"payload"`
	CoordinatorEpoch string `json:"coordinatorEpoch,omitempty"`
	FromIncarnation  string `json:"fromIncarnation,omitempty"`
	ToIncarnation    string `json:"toIncarnation,omitempty"`
}
type APIError struct {
	Status     int           `json:"-"`
	RetryAfter time.Duration `json:"-"`
	Code       string        `json:"code"`
	Message    string        `json:"message"`
}

func (e *APIError) Error() string {
	return fmt.Sprintf("cybergrid %s (%d): %s", e.Code, e.Status, e.Message)
}

type Config struct {
	AuthProvider AuthProvider
	BaseURL      string
	// AppID is the case-sensitive public Logto Native client_id (1..21 characters).
	AppID      string
	HTTPClient *http.Client
	// DirectTimeout applies to every direct attempt before tunnel fallback.
	DirectTimeout time.Duration
	// FallbackTimeout includes waiting for a remote provider and tunnel authentication.
	FallbackTimeout time.Duration
	PollInterval    time.Duration
	STUNServers     []string
	// DisableTunnel prevents spawning a local provider. Dialing remote tunnels remains possible.
	DisableTunnel  bool
	TunnelProvider TunnelProvider
	// OnError reports asynchronous signaling, heartbeat and provider errors. It must return promptly.
	OnError func(error)
	// OnTransportEvent reports connection stages without payloads or credentials.
	OnTransportEvent func(TransportEvent)
	// InterfaceName restricts ICE candidates to an interface, e.g. en0 on Mac.
	InterfaceName string
}
type Tunnel struct {
	URL   string
	Close func() error
	Done  <-chan error
}
type TunnelProvider interface {
	Start(context.Context, string) (*Tunnel, error)
}

func payload(v any) string { b, _ := json.Marshal(v); return string(b) }

// TransportEvent is intended for diagnostics; it never contains payloads, SDP or secrets.
type TransportEvent struct {
	Phase      string `json:"phase"`
	Transport  string `json:"transport"`
	PeerID     string `json:"peerId,omitempty"`
	Generation uint64 `json:"generation,omitempty"`
	Epoch      string `json:"epoch,omitempty"`
	Reason     string `json:"reason,omitempty"`
}

// AuthProvider owns private credentials and commits refresh rotation before return.
// It must never open a browser automatically. Calls are serialized by the SDK.
type AuthProvider interface {
	AccessToken(context.Context, *Client) (OAuthTokens, error)
}
type AuthProviderFunc func(context.Context, *Client) (OAuthTokens, error)

func (f AuthProviderFunc) AccessToken(ctx context.Context, c *Client) (OAuthTokens, error) {
	return f(ctx, c)
}

type Coordination struct {
	Protocol             string `json:"protocol"`
	Epoch                string `json:"epoch"`
	HeartbeatSeconds     int    `json:"heartbeatSeconds"`
	LeaseSeconds         int    `json:"leaseSeconds"`
	SignalTtlSeconds     int    `json:"signalTtlSeconds"`
	MaxSignalWaitSeconds int    `json:"maxSignalWaitSeconds"`
}
