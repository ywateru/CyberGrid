package cybergrid

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"testing"
)

func TestGrantRotationCheckpointFailureRetainsLatestAndPreventsStaleRefresh(t *testing.T) {
	c, f := newOAuthFixture(t)
	calls := 0
	f.grant = func(w http.ResponseWriter, r *http.Request, _ int) {
		calls++
		json.NewEncoder(w).Encode(map[string]any{"access_token": "resource", "refresh_token": "latest-rotation", "expires_in": 300, "token_type": "Bearer"})
	}
	c.SetGrantCheckpoint(func(context.Context, *Client, OAuthTokens) error { return errors.New("private-disk-path-secret") })
	_, err := c.RefreshAccessToken(context.Background(), "old-grant")
	if err == nil || strings.Contains(err.Error(), "private-disk-path-secret") || !c.HasPendingGrant() {
		t.Fatal("failed checkpoint did not retain sanitized latest grant")
	}
	var saved string
	c.SetGrantCheckpoint(func(_ context.Context, _ *Client, t OAuthTokens) error { saved = t.RefreshToken; return nil })
	if _, err = c.RefreshAccessToken(context.Background(), "old-grant"); err == nil {
		t.Fatal("old caller grant refreshed after recovering newer pending grant")
	}
	if calls != 1 || saved != "latest-rotation" || c.HasPendingGrant() {
		t.Fatal("latest rotation was not checkpointed before rejecting stale caller")
	}
	if _, err = c.RefreshAccessToken(context.Background(), saved); err != nil {
		t.Fatal(err)
	}
}
func TestDeviceMintCancellationSavesBothGrantsAndResourceFailurePreservesInitial(t *testing.T) {
	for _, failResource := range []bool{false, true} {
		t.Run(map[bool]string{false: "cancel-after-mint", true: "resource-failure"}[failResource], func(t *testing.T) {
			c, f := newOAuthFixture(t)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			var saved []string
			c.SetGrantCheckpoint(func(saveCtx context.Context, _ *Client, grant OAuthTokens) error {
				if saveCtx.Err() != nil {
					t.Error("checkpoint received canceled context")
				}
				saved = append(saved, grant.RefreshToken)
				return nil
			})
			f.grant = func(w http.ResponseWriter, r *http.Request, _ int) {
				if r.Form.Get("grant_type") == "refresh_token" {
					if failResource {
						w.WriteHeader(503)
						w.Write([]byte(`{"error":"server_error"}`))
						return
					}
					json.NewEncoder(w).Encode(map[string]any{"access_token": "resource", "refresh_token": "rotated", "expires_in": 300, "token_type": "Bearer"})
					return
				}
				cancel()
				json.NewEncoder(w).Encode(map[string]any{"access_token": "opaque", "refresh_token": "minted", "expires_in": 300, "token_type": "Bearer"})
			}
			auth, err := c.BeginDeviceAuthorization(ctx)
			if err != nil {
				t.Fatal(err)
			}
			tokens, err := c.CompleteDeviceAuthorization(ctx, auth)
			if failResource {
				if err == nil || len(saved) != 1 || saved[0] != "minted" {
					t.Fatal("initial grant lost on resource failure")
				}
			} else if err != nil || tokens.RefreshToken != "rotated" || len(saved) != 2 || saved[1] != "rotated" {
				t.Fatal("cancellation after mint lost rotation")
			}
		})
	}
}
