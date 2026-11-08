package main

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
)

func TestCLIRequiresExplicitAppAndHonorsFlagOverEnvironment(t *testing.T) {
	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { requests.Add(1); w.WriteHeader(500) }))
	defer server.Close()
	path := filepath.Join(t.TempDir(), "existing.json")
	if err := os.WriteFile(path, []byte(`{"version":2}`), 0600); err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		name, env string
		args      []string
		errorPart string
	}{
		{"missing-app", "", []string{"apps"}, "1..21"},
		{"oversized-env", strings.Repeat("A", 22), []string{"apps"}, "1..21"},
		{"explicit-overrides-env", strings.Repeat("A", 22), []string{"login", "-app", "NativeCaseA"}, "credential file already exists"},
		{"environment-app", "NativeCaseA", []string{"login"}, "credential file already exists"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			t.Setenv("CYBERGRID_APP_ID", tc.env)
			previous := os.Args
			defer func() { os.Args = previous }()
			os.Args = append([]string{"cybergrid"}, tc.args...)
			os.Args = append(os.Args, "-server", server.URL, "-credentials", path)
			if err := run(); err == nil || !strings.Contains(err.Error(), tc.errorPart) {
				t.Fatalf("unexpected command result: %v", err)
			}
			if requests.Load() != 0 {
				t.Fatal("invalid app or existing grant caused network activity")
			}
		})
	}
	t.Setenv("CYBERGRID_APP_ID", "")
	previous := os.Args
	defer func() { os.Args = previous }()
	os.Args = []string{"cybergrid", "version"}
	if err := run(); err != nil {
		t.Fatal("version unexpectedly requires application ID")
	}
}
