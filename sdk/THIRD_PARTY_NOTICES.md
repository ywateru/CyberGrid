# Third-party notices

The files under `third_party/` reproduce the original license, notice and
patent texts from every module in `go.mod`/`go.sum`'s resolved module graph,
including transitive and test dependencies, plus the Go runtime.
`third_party/manifest.json` records exact module versions and text hashes.

A transitive Pion dependency includes TURN implementation code; CyberGrid
does not configure or use TURN relaying. The Android CLI dynamically uses
the device's bionic system libraries; it does not bundle Android system
libraries or the NDK. cloudflared is a separate client-side program bundled
as fixed platform resources under provider-assets/, with its original
license, linked-module notices, source/patch pins and offline packaging.

Original CyberGrid code has no separately granted open-source license;
see LICENSE. Third-party license grants remain in effect.
