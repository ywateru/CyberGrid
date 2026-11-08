package cybergrid

import (
	"context"
	"errors"
	"time"
)

// GrantCheckpoint must take private ownership or durably save the latest refresh
// grant before returning nil. It must not log or serialize it into UI/errors.
// It is called under the client's grant mutex; do not recursively call OAuth APIs.
type GrantCheckpoint func(context.Context, *Client, OAuthTokens) error

func (c *Client) SetGrantCheckpoint(checkpoint GrantCheckpoint) {
	c.grantMu.Lock()
	defer c.grantMu.Unlock()
	c.grantCheckpoint = checkpoint
}
func (c *Client) checkpointGrant(ctx context.Context, t OAuthTokens) error {
	if t.RefreshToken == "" {
		return errors.New("refresh grant missing")
	}
	copy := t
	c.pendingGrant = &copy
	if c.grantCheckpoint == nil {
		return nil
	}
	return c.savePending(ctx)
}
func (c *Client) savePending(ctx context.Context) error {
	if c.pendingGrant == nil {
		return nil
	}
	if c.grantCheckpoint == nil {
		return errors.New("explicit private checkpoint required")
	}
	saveCtx, cancel := context.WithTimeout(context.WithoutCancel(ctx), 30*time.Second)
	defer cancel()
	if err := c.grantCheckpoint(saveCtx, c, *c.pendingGrant); err != nil {
		return errors.New("private grant checkpoint failed; latest grant retained")
	}
	c.pendingGrant = nil
	return nil
}
func (c *Client) RetryPendingGrant(ctx context.Context) error {
	c.grantMu.Lock()
	defer c.grantMu.Unlock()
	return c.savePending(ctx)
}
func (c *Client) HasPendingGrant() bool {
	c.grantMu.Lock()
	defer c.grantMu.Unlock()
	return c.pendingGrant != nil
}

// AcknowledgePendingGrant is for explicit host-managed storage: call only after
// saving/taking private ownership of exactly this latest refresh grant.
func (c *Client) AcknowledgePendingGrant(refresh string) error {
	c.grantMu.Lock()
	defer c.grantMu.Unlock()
	if c.pendingGrant == nil {
		return nil
	}
	if refresh != c.pendingGrant.RefreshToken {
		return errors.New("latest pending grant differs")
	}
	c.pendingGrant = nil
	return nil
}
func (c *Client) RevokePendingGrant(ctx context.Context) error {
	c.grantMu.Lock()
	defer c.grantMu.Unlock()
	if c.pendingGrant == nil {
		return nil
	}
	if err := c.RevokeRefreshToken(ctx, c.pendingGrant.RefreshToken); err != nil {
		return err
	}
	c.pendingGrant = nil
	return nil
}
