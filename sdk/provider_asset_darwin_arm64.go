//go:build darwin && arm64

package cybergrid

import _ "embed"

//go:embed provider-assets/darwin-arm64/cloudflared
var embeddedMacProvider []byte

func macProviderBytes() []byte { return embeddedMacProvider }
