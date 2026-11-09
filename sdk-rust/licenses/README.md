# Dependency license and notice texts

Verbatim files from the Cargo registry cache for the locked SDK dependency graph reachable on macOS arm64 or Android arm64. Includes build/dev dependencies as a conservative superset. Each package has SOURCE.json with crate checksum, Cargo source, available upstream commit and per-text SHA256. MANIFEST.json is the complete machine-readable index.

Some packages omit license texts from their crate archives. For those packages, texts were retrieved only from repository roots at the exact upstream commits recorded in those archives; SOURCE.json records immutable URLs. Other texts came from the Cargo cache. enum-assoc 1.4.2 provides no original license text at its exact published commit; this distribution selects its declared Apache-2.0 alternative and includes explicitly labelled canonical standard text and its verbatim exact-commit Cargo declaration. SOURCE.json records that exception without inventing copyright notices. Original bytes and nested paths are preserved, including COPYRIGHT, NOTICE, AUTHORS and nested LICENSES directories.

The original THIRD_PARTY_NOTICES.md retains the wider all-target Cargo.lock metadata list; packages exclusive to other architectures/OSes are outside this supported-target text collection. cloudflared is a separate provider and its notices belong to its own distribution.

| Package | Declared license | Text files |
| --- | --- | --- |
| aead-0.5.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](aead-0.5.2/LICENSE-APACHE), [LICENSE-MIT](aead-0.5.2/LICENSE-MIT) |
| aes-0.8.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](aes-0.8.4/LICENSE-APACHE), [LICENSE-MIT](aes-0.8.4/LICENSE-MIT) |
| aes-gcm-0.10.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](aes-gcm-0.10.3/LICENSE-APACHE), [LICENSE-MIT](aes-gcm-0.10.3/LICENSE-MIT) |
| aho-corasick-1.1.5 | Unlicense OR MIT | [COPYING](aho-corasick-1.1.5/COPYING), [LICENSE-MIT](aho-corasick-1.1.5/LICENSE-MIT), [UNLICENSE](aho-corasick-1.1.5/UNLICENSE) |
| android_system_properties-0.1.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](android_system_properties-0.1.6/LICENSE-APACHE), [LICENSE-MIT](android_system_properties-0.1.6/LICENSE-MIT) |
| arc-swap-1.9.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](arc-swap-1.9.3/LICENSE-APACHE), [LICENSE-MIT](arc-swap-1.9.3/LICENSE-MIT) |
| arrayvec-0.7.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](arrayvec-0.7.8/LICENSE-APACHE), [LICENSE-MIT](arrayvec-0.7.8/LICENSE-MIT) |
| asn1-rs-0.6.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](asn1-rs-0.6.2/LICENSE-APACHE), [LICENSE-MIT](asn1-rs-0.6.2/LICENSE-MIT) |
| asn1-rs-0.7.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](asn1-rs-0.7.2/LICENSE-APACHE), [LICENSE-MIT](asn1-rs-0.7.2/LICENSE-MIT) |
| asn1-rs-derive-0.5.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](asn1-rs-derive-0.5.1/LICENSE-APACHE), [LICENSE-MIT](asn1-rs-derive-0.5.1/LICENSE-MIT) |
| asn1-rs-derive-0.6.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](asn1-rs-derive-0.6.0/LICENSE-APACHE), [LICENSE-MIT](asn1-rs-derive-0.6.0/LICENSE-MIT) |
| asn1-rs-impl-0.2.0 | MIT/Apache-2.0 | [LICENSE-MIT](asn1-rs-impl-0.2.0/upstream-repository-root/LICENSE-MIT), [LICENSE-APACHE](asn1-rs-impl-0.2.0/upstream-repository-root/LICENSE-APACHE) |
| async-trait-0.1.92 | MIT OR Apache-2.0 | [LICENSE-APACHE](async-trait-0.1.92/LICENSE-APACHE), [LICENSE-MIT](async-trait-0.1.92/LICENSE-MIT) |
| atomic-waker-1.1.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](atomic-waker-1.1.2/LICENSE-APACHE), [LICENSE-MIT](atomic-waker-1.1.2/LICENSE-MIT), [LICENSE-THIRD-PARTY](atomic-waker-1.1.2/LICENSE-THIRD-PARTY) |
| autocfg-1.5.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](autocfg-1.5.1/LICENSE-APACHE), [LICENSE-MIT](autocfg-1.5.1/LICENSE-MIT) |
| axum-0.8.9 | MIT | [LICENSE](axum-0.8.9/LICENSE) |
| axum-core-0.5.6 | MIT | [LICENSE](axum-core-0.5.6/LICENSE) |
| backon-1.6.0 | Apache-2.0 | [LICENSE](backon-1.6.0/LICENSE) |
| base16ct-0.2.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](base16ct-0.2.0/LICENSE-APACHE), [LICENSE-MIT](base16ct-0.2.0/LICENSE-MIT) |
| base16ct-1.0.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](base16ct-1.0.0/LICENSE-APACHE), [LICENSE-MIT](base16ct-1.0.0/LICENSE-MIT) |
| base64-0.22.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](base64-0.22.1/LICENSE-APACHE), [LICENSE-MIT](base64-0.22.1/LICENSE-MIT) |
| base64-0.23.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](base64-0.23.1/LICENSE-APACHE), [LICENSE-MIT](base64-0.23.1/LICENSE-MIT) |
| base64ct-1.8.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](base64ct-1.8.3/LICENSE-APACHE), [LICENSE-MIT](base64ct-1.8.3/LICENSE-MIT) |
| bincode-1.3.3 | MIT | [LICENSE.md](bincode-1.3.3/LICENSE.md) |
| bit-vec-0.9.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](bit-vec-0.9.1/LICENSE-APACHE), [LICENSE-MIT](bit-vec-0.9.1/LICENSE-MIT) |
| bitflags-1.3.2 | MIT/Apache-2.0 | [LICENSE-APACHE](bitflags-1.3.2/LICENSE-APACHE), [LICENSE-MIT](bitflags-1.3.2/LICENSE-MIT) |
| bitflags-2.13.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](bitflags-2.13.2/LICENSE-APACHE), [LICENSE-MIT](bitflags-2.13.2/LICENSE-MIT) |
| blake3-1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | [LICENSE_A2](blake3-1.8.7/LICENSE_A2), [LICENSE_A2LLVM](blake3-1.8.7/LICENSE_A2LLVM), [LICENSE_CC0](blake3-1.8.7/LICENSE_CC0) |
| block-buffer-0.10.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](block-buffer-0.10.4/LICENSE-APACHE), [LICENSE-MIT](block-buffer-0.10.4/LICENSE-MIT) |
| block-buffer-0.12.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](block-buffer-0.12.1/LICENSE-APACHE), [LICENSE-MIT](block-buffer-0.12.1/LICENSE-MIT) |
| block-padding-0.3.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](block-padding-0.3.3/LICENSE-APACHE), [LICENSE-MIT](block-padding-0.3.3/LICENSE-MIT) |
| block2-0.6.2 | MIT | [LICENSE.md](block2-0.6.2/upstream-repository-root/LICENSE.md) |
| byteorder-1.5.0 | Unlicense OR MIT | [COPYING](byteorder-1.5.0/COPYING), [LICENSE-MIT](byteorder-1.5.0/LICENSE-MIT), [UNLICENSE](byteorder-1.5.0/UNLICENSE) |
| bytes-1.12.1 | MIT | [LICENSE](bytes-1.12.1/LICENSE) |
| cbc-0.1.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](cbc-0.1.2/LICENSE-APACHE), [LICENSE-MIT](cbc-0.1.2/LICENSE-MIT) |
| cc-1.11.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](cc-1.11.1/LICENSE-APACHE), [LICENSE-MIT](cc-1.11.1/LICENSE-MIT) |
| ccm-0.5.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](ccm-0.5.0/LICENSE-APACHE), [LICENSE-MIT](ccm-0.5.0/LICENSE-MIT) |
| cesu8-1.1.0 | Apache-2.0/MIT | [COPYRIGHT-RUST.txt](cesu8-1.1.0/COPYRIGHT-RUST.txt) |
| cfg-if-1.0.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](cfg-if-1.0.5/LICENSE-APACHE), [LICENSE-MIT](cfg-if-1.0.5/LICENSE-MIT) |
| cfg_aliases-0.2.2 | MIT | [LICENSE](cfg_aliases-0.2.2/LICENSE) |
| chacha20-0.10.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](chacha20-0.10.2/LICENSE-APACHE), [LICENSE-MIT](chacha20-0.10.2/LICENSE-MIT) |
| chacha20-0.9.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](chacha20-0.9.1/LICENSE-APACHE), [LICENSE-MIT](chacha20-0.9.1/LICENSE-MIT) |
| chacha20poly1305-0.10.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](chacha20poly1305-0.10.1/LICENSE-APACHE), [LICENSE-MIT](chacha20poly1305-0.10.1/LICENSE-MIT) |
| chrono-0.4.45 | MIT OR Apache-2.0 | [LICENSE.txt](chrono-0.4.45/LICENSE.txt) |
| cipher-0.4.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](cipher-0.4.4/LICENSE-APACHE), [LICENSE-MIT](cipher-0.4.4/LICENSE-MIT) |
| cmov-0.5.4 | Apache-2.0 OR MIT | [LICENSE-APACHE](cmov-0.5.4/LICENSE-APACHE), [LICENSE-MIT](cmov-0.5.4/LICENSE-MIT) |
| cobs-0.3.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](cobs-0.3.0/LICENSE-APACHE), [LICENSE-MIT](cobs-0.3.0/LICENSE-MIT) |
| combine-4.6.9 | MIT | [LICENSE](combine-4.6.9/LICENSE) |
| const-oid-0.10.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](const-oid-0.10.2/LICENSE-APACHE), [LICENSE-MIT](const-oid-0.10.2/LICENSE-MIT) |
| const-oid-0.9.6 | Apache-2.0 OR MIT | [LICENSE-APACHE](const-oid-0.9.6/LICENSE-APACHE), [LICENSE-MIT](const-oid-0.9.6/LICENSE-MIT) |
| constant_time_eq-0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 | [LICENSE-APACHE](constant_time_eq-0.4.2/LICENSE-APACHE), [LICENSE-CC0](constant_time_eq-0.4.2/LICENSE-CC0), [LICENSE-MIT0](constant_time_eq-0.4.2/LICENSE-MIT0) |
| convert_case-0.10.0 | MIT | [LICENSE](convert_case-0.10.0/LICENSE) |
| cordyceps-0.3.5 | MIT | [LICENSE](cordyceps-0.3.5/LICENSE) |
| core-foundation-0.10.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](core-foundation-0.10.1/LICENSE-APACHE), [LICENSE-MIT](core-foundation-0.10.1/LICENSE-MIT) |
| core-foundation-sys-0.8.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](core-foundation-sys-0.8.7/LICENSE-APACHE), [LICENSE-MIT](core-foundation-sys-0.8.7/LICENSE-MIT) |
| cpufeatures-0.2.17 | MIT OR Apache-2.0 | [LICENSE-APACHE](cpufeatures-0.2.17/LICENSE-APACHE), [LICENSE-MIT](cpufeatures-0.2.17/LICENSE-MIT) |
| cpufeatures-0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](cpufeatures-0.3.1/LICENSE-APACHE), [LICENSE-MIT](cpufeatures-0.3.1/LICENSE-MIT) |
| crc-3.4.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](crc-3.4.0/LICENSE-APACHE), [LICENSE-MIT](crc-3.4.0/LICENSE-MIT) |
| crc-catalog-2.5.0 | MIT OR Apache-2.0 | [Apache-2.0.txt](crc-catalog-2.5.0/LICENSES/Apache-2.0.txt), [MIT.txt](crc-catalog-2.5.0/LICENSES/MIT.txt) |
| crypto-bigint-0.5.5 | Apache-2.0 OR MIT | [LICENSE-APACHE](crypto-bigint-0.5.5/LICENSE-APACHE), [LICENSE-MIT](crypto-bigint-0.5.5/LICENSE-MIT) |
| crypto-common-0.1.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](crypto-common-0.1.7/LICENSE-APACHE), [LICENSE-MIT](crypto-common-0.1.7/LICENSE-MIT) |
| crypto-common-0.2.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](crypto-common-0.2.2/LICENSE-APACHE), [LICENSE-MIT](crypto-common-0.2.2/LICENSE-MIT) |
| ctr-0.9.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](ctr-0.9.2/LICENSE-APACHE), [LICENSE-MIT](ctr-0.9.2/LICENSE-MIT) |
| ctutils-0.4.8 | Apache-2.0 OR MIT | [LICENSE-APACHE](ctutils-0.4.8/LICENSE-APACHE), [LICENSE-MIT](ctutils-0.4.8/LICENSE-MIT) |
| curve25519-dalek-4.1.3 | BSD-3-Clause | [LICENSE](curve25519-dalek-4.1.3/LICENSE) |
| curve25519-dalek-5.0.0 | BSD-3-Clause | [LICENSE](curve25519-dalek-5.0.0/LICENSE) |
| data-encoding-2.11.1 | MIT | [LICENSE](data-encoding-2.11.1/LICENSE) |
| data-encoding-macro-0.1.21 | MIT | [LICENSE](data-encoding-macro-0.1.21/LICENSE) |
| data-encoding-macro-internal-0.1.19 | MIT | [LICENSE](data-encoding-macro-internal-0.1.19/LICENSE) |
| der-0.7.10 | Apache-2.0 OR MIT | [LICENSE-APACHE](der-0.7.10/LICENSE-APACHE), [LICENSE-MIT](der-0.7.10/LICENSE-MIT) |
| der-0.8.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](der-0.8.3/LICENSE-APACHE), [LICENSE-MIT](der-0.8.3/LICENSE-MIT) |
| der-parser-10.0.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](der-parser-10.0.0/LICENSE-APACHE), [LICENSE-MIT](der-parser-10.0.0/LICENSE-MIT) |
| der-parser-9.0.0 | MIT/Apache-2.0 | [LICENSE-APACHE](der-parser-9.0.0/LICENSE-APACHE), [LICENSE-MIT](der-parser-9.0.0/LICENSE-MIT) |
| deranged-0.5.9 | MIT OR Apache-2.0 | [LICENSE-Apache](deranged-0.5.9/LICENSE-Apache), [LICENSE-MIT](deranged-0.5.9/LICENSE-MIT) |
| derive_more-2.1.1 | MIT | [LICENSE](derive_more-2.1.1/LICENSE) |
| derive_more-impl-2.1.1 | MIT | [LICENSE](derive_more-impl-2.1.1/LICENSE) |
| diatomic-waker-0.2.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](diatomic-waker-0.2.3/LICENSE-APACHE), [LICENSE-MIT](diatomic-waker-0.2.3/LICENSE-MIT) |
| digest-0.10.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](digest-0.10.7/LICENSE-APACHE), [LICENSE-MIT](digest-0.10.7/LICENSE-MIT) |
| digest-0.11.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](digest-0.11.3/LICENSE-APACHE), [LICENSE-MIT](digest-0.11.3/LICENSE-MIT) |
| dispatch2-0.3.1 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md](dispatch2-0.3.1/upstream-repository-root/LICENSE.md) |
| displaydoc-0.2.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](displaydoc-0.2.7/LICENSE-APACHE), [LICENSE-MIT](displaydoc-0.2.7/LICENSE-MIT) |
| dlopen2-0.8.2 | MIT | [LICENSE](dlopen2-0.8.2/upstream-repository-root/LICENSE) |
| dtls-0.13.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](dtls-0.13.0/LICENSE-APACHE), [LICENSE-MIT](dtls-0.13.0/LICENSE-MIT) |
| ecdsa-0.16.9 | Apache-2.0 OR MIT | [LICENSE-APACHE](ecdsa-0.16.9/LICENSE-APACHE), [LICENSE-MIT](ecdsa-0.16.9/LICENSE-MIT) |
| ed25519-3.0.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](ed25519-3.0.0/LICENSE-APACHE), [LICENSE-MIT](ed25519-3.0.0/LICENSE-MIT) |
| ed25519-dalek-3.0.0 | BSD-3-Clause | [LICENSE](ed25519-dalek-3.0.0/LICENSE) |
| elliptic-curve-0.13.8 | Apache-2.0 OR MIT | [LICENSE-APACHE](elliptic-curve-0.13.8/LICENSE-APACHE), [LICENSE-MIT](elliptic-curve-0.13.8/LICENSE-MIT) |
| embedded-io-0.4.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](embedded-io-0.4.0/LICENSE-APACHE), [LICENSE-MIT](embedded-io-0.4.0/LICENSE-MIT) |
| embedded-io-0.6.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](embedded-io-0.6.1/LICENSE-APACHE), [LICENSE-MIT](embedded-io-0.6.1/LICENSE-MIT) |
| enum-assoc-1.4.2 | MIT OR Apache-2.0 | [Apache-2.0.txt](enum-assoc-1.4.2/standard-license-text/Apache-2.0.txt), [Cargo.toml.orig](enum-assoc-1.4.2/declared-license-source/Cargo.toml.orig) |
| equivalent-1.0.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](equivalent-1.0.3/LICENSE-APACHE), [LICENSE-MIT](equivalent-1.0.3/LICENSE-MIT) |
| errno-0.3.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](errno-0.3.14/LICENSE-APACHE), [LICENSE-MIT](errno-0.3.14/LICENSE-MIT) |
| fastrand-2.5.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](fastrand-2.5.0/LICENSE-APACHE), [LICENSE-MIT](fastrand-2.5.0/LICENSE-MIT) |
| ff-0.13.1 | MIT/Apache-2.0 | [LICENSE-APACHE](ff-0.13.1/LICENSE-APACHE), [LICENSE-MIT](ff-0.13.1/LICENSE-MIT) |
| find-msvc-tools-0.1.15 | MIT OR Apache-2.0 | [LICENSE-APACHE](find-msvc-tools-0.1.15/LICENSE-APACHE), [LICENSE-MIT](find-msvc-tools-0.1.15/LICENSE-MIT) |
| fixedbitset-0.5.9 | MIT OR Apache-2.0 | [LICENSE-APACHE](fixedbitset-0.5.9/LICENSE-APACHE), [LICENSE-MIT](fixedbitset-0.5.9/LICENSE-MIT) |
| foldhash-0.2.0 | Zlib | [LICENSE](foldhash-0.2.0/LICENSE) |
| form_urlencoded-1.2.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](form_urlencoded-1.2.2/LICENSE-APACHE), [LICENSE-MIT](form_urlencoded-1.2.2/LICENSE-MIT) |
| fs2-0.4.3 | MIT/Apache-2.0 | [LICENSE-APACHE](fs2-0.4.3/LICENSE-APACHE), [LICENSE-MIT](fs2-0.4.3/LICENSE-MIT) |
| futures-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-0.3.34/LICENSE-MIT) |
| futures-buffered-0.2.14 | MIT | [LICENSE](futures-buffered-0.2.14/LICENSE) |
| futures-channel-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-channel-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-channel-0.3.34/LICENSE-MIT) |
| futures-core-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-core-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-core-0.3.34/LICENSE-MIT) |
| futures-executor-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-executor-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-executor-0.3.34/LICENSE-MIT) |
| futures-io-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-io-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-io-0.3.34/LICENSE-MIT) |
| futures-lite-2.6.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](futures-lite-2.6.1/LICENSE-APACHE), [LICENSE-MIT](futures-lite-2.6.1/LICENSE-MIT), [LICENSE-THIRD-PARTY](futures-lite-2.6.1/LICENSE-THIRD-PARTY) |
| futures-macro-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-macro-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-macro-0.3.34/LICENSE-MIT) |
| futures-sink-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-sink-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-sink-0.3.34/LICENSE-MIT) |
| futures-task-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-task-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-task-0.3.34/LICENSE-MIT) |
| futures-util-0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](futures-util-0.3.34/LICENSE-APACHE), [LICENSE-MIT](futures-util-0.3.34/LICENSE-MIT) |
| generic-array-0.14.7 | MIT | [LICENSE](generic-array-0.14.7/LICENSE) |
| getrandom-0.2.17 | MIT OR Apache-2.0 | [LICENSE-APACHE](getrandom-0.2.17/LICENSE-APACHE), [LICENSE-MIT](getrandom-0.2.17/LICENSE-MIT) |
| getrandom-0.3.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](getrandom-0.3.4/LICENSE-APACHE), [LICENSE-MIT](getrandom-0.3.4/LICENSE-MIT) |
| getrandom-0.4.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](getrandom-0.4.3/LICENSE-APACHE), [LICENSE-MIT](getrandom-0.4.3/LICENSE-MIT) |
| ghash-0.5.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](ghash-0.5.1/LICENSE-APACHE), [LICENSE-MIT](ghash-0.5.1/LICENSE-MIT) |
| group-0.13.0 | MIT/Apache-2.0 | [COPYRIGHT](group-0.13.0/COPYRIGHT), [LICENSE-APACHE](group-0.13.0/LICENSE-APACHE), [LICENSE-MIT](group-0.13.0/LICENSE-MIT) |
| hashbrown-0.17.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](hashbrown-0.17.1/LICENSE-APACHE), [LICENSE-MIT](hashbrown-0.17.1/LICENSE-MIT) |
| heck-0.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](heck-0.5.0/LICENSE-APACHE), [LICENSE-MIT](heck-0.5.0/LICENSE-MIT) |
| hex-0.4.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](hex-0.4.3/LICENSE-APACHE), [LICENSE-MIT](hex-0.4.3/LICENSE-MIT) |
| hkdf-0.12.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](hkdf-0.12.4/LICENSE-APACHE), [LICENSE-MIT](hkdf-0.12.4/LICENSE-MIT) |
| hmac-0.12.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](hmac-0.12.1/LICENSE-APACHE), [LICENSE-MIT](hmac-0.12.1/LICENSE-MIT) |
| http-1.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](http-1.5.0/LICENSE-APACHE), [LICENSE-MIT](http-1.5.0/LICENSE-MIT) |
| http-body-1.1.0 | MIT | [LICENSE](http-body-1.1.0/LICENSE) |
| http-body-util-0.1.5 | MIT | [LICENSE](http-body-util-0.1.5/LICENSE) |
| httparse-1.10.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](httparse-1.10.1/LICENSE-APACHE), [LICENSE-MIT](httparse-1.10.1/LICENSE-MIT) |
| httpdate-1.0.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](httpdate-1.0.3/LICENSE-APACHE), [LICENSE-MIT](httpdate-1.0.3/LICENSE-MIT) |
| hybrid-array-0.4.15 | MIT OR Apache-2.0 | [LICENSE-APACHE](hybrid-array-0.4.15/LICENSE-APACHE), [LICENSE-MIT](hybrid-array-0.4.15/LICENSE-MIT) |
| hyper-1.12.0 | MIT | [LICENSE](hyper-1.12.0/LICENSE) |
| hyper-rustls-0.27.10 | Apache-2.0 OR ISC OR MIT | [LICENSE-APACHE](hyper-rustls-0.27.10/LICENSE-APACHE), [LICENSE-ISC](hyper-rustls-0.27.10/LICENSE-ISC), [LICENSE-MIT](hyper-rustls-0.27.10/LICENSE-MIT) |
| hyper-util-0.1.21 | MIT | [LICENSE](hyper-util-0.1.21/LICENSE) |
| iana-time-zone-0.1.65 | MIT OR Apache-2.0 | [LICENSE-APACHE](iana-time-zone-0.1.65/LICENSE-APACHE), [LICENSE-MIT](iana-time-zone-0.1.65/LICENSE-MIT) |
| icu_collections-2.4.0 | Unicode-3.0 | [LICENSE](icu_collections-2.4.0/LICENSE) |
| icu_locale_core-2.4.0 | Unicode-3.0 | [LICENSE](icu_locale_core-2.4.0/LICENSE) |
| icu_normalizer-2.4.0 | Unicode-3.0 | [LICENSE](icu_normalizer-2.4.0/LICENSE) |
| icu_normalizer_data-2.4.0 | Unicode-3.0 | [LICENSE](icu_normalizer_data-2.4.0/LICENSE) |
| icu_properties-2.4.0 | Unicode-3.0 | [LICENSE](icu_properties-2.4.0/LICENSE) |
| icu_properties_data-2.4.0 | Unicode-3.0 | [LICENSE](icu_properties_data-2.4.0/LICENSE) |
| icu_provider-2.4.0 | Unicode-3.0 | [LICENSE](icu_provider-2.4.0/LICENSE) |
| identity-hash-0.1.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](identity-hash-0.1.0/LICENSE-APACHE), [LICENSE-MIT](identity-hash-0.1.0/LICENSE-MIT) |
| idna-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](idna-1.1.0/LICENSE-APACHE), [LICENSE-MIT](idna-1.1.0/LICENSE-MIT) |
| idna_adapter-1.2.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](idna_adapter-1.2.2/LICENSE-APACHE), [LICENSE-MIT](idna_adapter-1.2.2/LICENSE-MIT) |
| indexmap-2.14.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](indexmap-2.14.2/LICENSE-APACHE), [LICENSE-MIT](indexmap-2.14.2/LICENSE-MIT) |
| inout-0.1.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](inout-0.1.4/LICENSE-APACHE), [LICENSE-MIT](inout-0.1.4/LICENSE-MIT) |
| interceptor-0.15.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](interceptor-0.15.0/LICENSE-APACHE), [LICENSE-MIT](interceptor-0.15.0/LICENSE-MIT) |
| ipnet-2.12.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](ipnet-2.12.2/LICENSE-APACHE), [LICENSE-MIT](ipnet-2.12.2/LICENSE-MIT) |
| iroh-1.4.1 | MIT OR Apache-2.0 | [LICENSE-BSD3](iroh-1.4.1/LICENSE-BSD3), [LICENSE-APACHE](iroh-1.4.1/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](iroh-1.4.1/upstream-repository-root/LICENSE-MIT) |
| iroh-base-1.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](iroh-base-1.4.1/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](iroh-base-1.4.1/upstream-repository-root/LICENSE-MIT) |
| iroh-dns-1.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](iroh-dns-1.4.1/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](iroh-dns-1.4.1/upstream-repository-root/LICENSE-MIT) |
| iroh-metrics-1.0.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](iroh-metrics-1.0.2/LICENSE-APACHE), [LICENSE-MIT](iroh-metrics-1.0.2/LICENSE-MIT) |
| iroh-metrics-derive-1.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](iroh-metrics-derive-1.0.1/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](iroh-metrics-derive-1.0.1/upstream-repository-root/LICENSE-MIT) |
| iroh-relay-1.4.1 | MIT OR Apache-2.0 | [LICENSE-BSD3](iroh-relay-1.4.1/LICENSE-BSD3), [LICENSE-APACHE](iroh-relay-1.4.1/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](iroh-relay-1.4.1/upstream-repository-root/LICENSE-MIT) |
| itoa-1.0.18 | MIT OR Apache-2.0 | [LICENSE-APACHE](itoa-1.0.18/LICENSE-APACHE), [LICENSE-MIT](itoa-1.0.18/LICENSE-MIT) |
| jni-0.21.1 | MIT/Apache-2.0 | [LICENSE-APACHE](jni-0.21.1/LICENSE-APACHE), [LICENSE-MIT](jni-0.21.1/LICENSE-MIT) |
| jni-0.22.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](jni-0.22.4/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](jni-0.22.4/upstream-repository-root/LICENSE-MIT) |
| jni-macros-0.22.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](jni-macros-0.22.4/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](jni-macros-0.22.4/upstream-repository-root/LICENSE-MIT) |
| jni-sys-0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](jni-sys-0.3.1/LICENSE-APACHE), [LICENSE-MIT](jni-sys-0.3.1/LICENSE-MIT) |
| jni-sys-0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](jni-sys-0.4.1/LICENSE-APACHE), [LICENSE-MIT](jni-sys-0.4.1/LICENSE-MIT) |
| jni-sys-macros-0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](jni-sys-macros-0.4.1/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](jni-sys-macros-0.4.1/upstream-repository-root/LICENSE-MIT) |
| lazy_static-1.5.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](lazy_static-1.5.1/LICENSE-APACHE), [LICENSE-MIT](lazy_static-1.5.1/LICENSE-MIT) |
| libc-0.2.191 | MIT OR Apache-2.0 | [LICENSE-APACHE](libc-0.2.191/LICENSE-APACHE), [LICENSE-MIT](libc-0.2.191/LICENSE-MIT) |
| linux-raw-sys-0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [COPYRIGHT](linux-raw-sys-0.12.1/COPYRIGHT), [LICENSE-APACHE](linux-raw-sys-0.12.1/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](linux-raw-sys-0.12.1/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](linux-raw-sys-0.12.1/LICENSE-MIT) |
| litemap-0.8.3 | Unicode-3.0 | [LICENSE](litemap-0.8.3/LICENSE) |
| lock_api-0.4.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](lock_api-0.4.14/LICENSE-APACHE), [LICENSE-MIT](lock_api-0.4.14/LICENSE-MIT) |
| log-0.4.35 | MIT OR Apache-2.0 | [LICENSE-APACHE](log-0.4.35/LICENSE-APACHE), [LICENSE-MIT](log-0.4.35/LICENSE-MIT) |
| lru-0.18.5 | MIT | [LICENSE](lru-0.18.5/LICENSE) |
| lru-slab-0.1.3 | MIT OR Apache-2.0 OR Zlib | [LICENSE-APACHE](lru-slab-0.1.3/LICENSE-APACHE), [LICENSE-MIT](lru-slab-0.1.3/LICENSE-MIT), [LICENSE-ZLIB](lru-slab-0.1.3/LICENSE-ZLIB) |
| mac-addr-0.3.0 | MIT | [LICENSE](mac-addr-0.3.0/LICENSE) |
| matchit-0.8.4 | MIT AND BSD-3-Clause | [LICENSE](matchit-0.8.4/LICENSE), [LICENSE.httprouter](matchit-0.8.4/LICENSE.httprouter) |
| md-5-0.10.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](md-5-0.10.6/LICENSE-APACHE), [LICENSE-MIT](md-5-0.10.6/LICENSE-MIT) |
| memchr-2.8.3 | Unlicense OR MIT | [COPYING](memchr-2.8.3/COPYING), [LICENSE-MIT](memchr-2.8.3/LICENSE-MIT), [UNLICENSE](memchr-2.8.3/UNLICENSE) |
| memoffset-0.7.1 | MIT | [LICENSE](memoffset-0.7.1/LICENSE) |
| mime-0.3.18 | MIT OR Apache-2.0 | [LICENSE-APACHE](mime-0.3.18/LICENSE-APACHE), [LICENSE-MIT](mime-0.3.18/LICENSE-MIT) |
| minimal-lexical-0.2.1 | MIT/Apache-2.0 | [LICENSE-APACHE](minimal-lexical-0.2.1/LICENSE-APACHE), [LICENSE-MIT](minimal-lexical-0.2.1/LICENSE-MIT), [LICENSE.md](minimal-lexical-0.2.1/LICENSE.md) |
| mio-1.2.4 | MIT | [LICENSE](mio-1.2.4/LICENSE) |
| n0-dns-resolver-0.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](n0-dns-resolver-0.1.0/LICENSE-APACHE), [LICENSE-MIT](n0-dns-resolver-0.1.0/LICENSE-MIT) |
| n0-error-1.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](n0-error-1.0.1/LICENSE-APACHE), [LICENSE-MIT](n0-error-1.0.1/LICENSE-MIT) |
| n0-error-macros-1.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](n0-error-macros-1.0.1/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](n0-error-macros-1.0.1/upstream-repository-root/LICENSE-MIT) |
| n0-future-0.3.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](n0-future-0.3.2/LICENSE-APACHE), [LICENSE-MIT](n0-future-0.3.2/LICENSE-MIT) |
| n0-watcher-1.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](n0-watcher-1.0.1/LICENSE-APACHE), [LICENSE-MIT](n0-watcher-1.0.1/LICENSE-MIT) |
| ndk-context-0.1.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](ndk-context-0.1.1/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](ndk-context-0.1.1/upstream-repository-root/LICENSE-MIT) |
| netdev-0.46.3 | MIT | [LICENSE](netdev-0.46.3/LICENSE) |
| netlink-packet-core-0.8.2 | MIT | [license.yml](netlink-packet-core-0.8.2/.github/workflows/license.yml), [LICENSE-MIT](netlink-packet-core-0.8.2/LICENSE-MIT) |
| netlink-packet-core-0.9.0 | MIT | [license.yml](netlink-packet-core-0.9.0/.github/workflows/license.yml), [LICENSE-MIT](netlink-packet-core-0.9.0/LICENSE-MIT) |
| netlink-packet-route-0.31.0 | MIT | [license.yml](netlink-packet-route-0.31.0/.github/workflows/license.yml), [LICENSE-MIT](netlink-packet-route-0.31.0/LICENSE-MIT) |
| netlink-packet-route-0.33.0 | MIT | [license.yml](netlink-packet-route-0.33.0/.github/workflows/license.yml), [LICENSE-MIT](netlink-packet-route-0.33.0/LICENSE-MIT) |
| netlink-proto-0.12.2 | MIT | [license.yml](netlink-proto-0.12.2/.github/workflows/license.yml), [LICENSE-MIT](netlink-proto-0.12.2/LICENSE-MIT) |
| netlink-sys-0.8.8 | MIT | [license.yml](netlink-sys-0.8.8/.github/workflows/license.yml), [LICENSE-MIT](netlink-sys-0.8.8/LICENSE-MIT) |
| netlink-sys-0.9.0 | MIT | [license.yml](netlink-sys-0.9.0/.github/workflows/license.yml), [LICENSE-MIT](netlink-sys-0.9.0/LICENSE-MIT) |
| netwatch-0.19.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](netwatch-0.19.3/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](netwatch-0.19.3/upstream-repository-root/LICENSE-MIT) |
| nix-0.26.4 | MIT | [LICENSE](nix-0.26.4/LICENSE) |
| nom-7.1.3 | MIT | [LICENSE](nom-7.1.3/LICENSE) |
| noq-1.4.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](noq-1.4.0/LICENSE-APACHE), [LICENSE-MIT](noq-1.4.0/LICENSE-MIT) |
| noq-proto-1.4.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](noq-proto-1.4.0/LICENSE-APACHE), [LICENSE-MIT](noq-proto-1.4.0/LICENSE-MIT) |
| noq-udp-1.4.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](noq-udp-1.4.0/LICENSE-APACHE), [LICENSE-MIT](noq-udp-1.4.0/LICENSE-MIT) |
| num-bigint-0.4.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](num-bigint-0.4.8/LICENSE-APACHE), [LICENSE-MIT](num-bigint-0.4.8/LICENSE-MIT) |
| num-conv-0.2.2 | MIT OR Apache-2.0 | [LICENSE-Apache](num-conv-0.2.2/LICENSE-Apache), [LICENSE-MIT](num-conv-0.2.2/LICENSE-MIT) |
| num-integer-0.1.47 | MIT OR Apache-2.0 | [LICENSE-APACHE](num-integer-0.1.47/LICENSE-APACHE), [LICENSE-MIT](num-integer-0.1.47/LICENSE-MIT) |
| num-traits-0.2.19 | MIT OR Apache-2.0 | [LICENSE-APACHE](num-traits-0.2.19/LICENSE-APACHE), [LICENSE-MIT](num-traits-0.2.19/LICENSE-MIT) |
| num_enum-0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | [LICENSE-APACHE](num_enum-0.7.6/LICENSE-APACHE), [LICENSE-BSD](num_enum-0.7.6/LICENSE-BSD), [LICENSE-MIT](num_enum-0.7.6/LICENSE-MIT) |
| num_enum_derive-0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | [LICENSE-APACHE](num_enum_derive-0.7.6/LICENSE-APACHE), [LICENSE-BSD](num_enum_derive-0.7.6/LICENSE-BSD), [LICENSE-MIT](num_enum_derive-0.7.6/LICENSE-MIT) |
| objc2-0.6.5 | MIT | [LICENSE.md](objc2-0.6.5/upstream-repository-root/LICENSE.md) |
| objc2-core-foundation-0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md](objc2-core-foundation-0.3.2/upstream-repository-root/LICENSE.md) |
| objc2-encode-4.1.0 | MIT | [LICENSE.md](objc2-encode-4.1.0/upstream-repository-root/LICENSE.md) |
| objc2-security-0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md](objc2-security-0.3.2/upstream-repository-root/LICENSE.md) |
| objc2-system-configuration-0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md](objc2-system-configuration-0.3.2/upstream-repository-root/LICENSE.md) |
| oid-registry-0.7.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](oid-registry-0.7.1/LICENSE-APACHE), [LICENSE-MIT](oid-registry-0.7.1/LICENSE-MIT) |
| oid-registry-0.8.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](oid-registry-0.8.1/LICENSE-APACHE), [LICENSE-MIT](oid-registry-0.8.1/LICENSE-MIT) |
| once_cell-1.21.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](once_cell-1.21.4/LICENSE-APACHE), [LICENSE-MIT](once_cell-1.21.4/LICENSE-MIT) |
| opaque-debug-0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](opaque-debug-0.3.1/LICENSE-APACHE), [LICENSE-MIT](opaque-debug-0.3.1/LICENSE-MIT) |
| p256-0.13.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](p256-0.13.2/LICENSE-APACHE), [LICENSE-MIT](p256-0.13.2/LICENSE-MIT) |
| p384-0.13.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](p384-0.13.1/LICENSE-APACHE), [LICENSE-MIT](p384-0.13.1/LICENSE-MIT) |
| papaya-0.2.5 | MIT | [LICENSE.md](papaya-0.2.5/LICENSE.md) |
| parking-2.2.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](parking-2.2.1/LICENSE-APACHE), [LICENSE-MIT](parking-2.2.1/LICENSE-MIT), [LICENSE-THIRD-PARTY](parking-2.2.1/LICENSE-THIRD-PARTY) |
| parking_lot-0.12.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](parking_lot-0.12.5/LICENSE-APACHE), [LICENSE-MIT](parking_lot-0.12.5/LICENSE-MIT) |
| parking_lot_core-0.9.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](parking_lot_core-0.9.12/LICENSE-APACHE), [LICENSE-MIT](parking_lot_core-0.9.12/LICENSE-MIT) |
| paste-1.0.15 | MIT OR Apache-2.0 | [LICENSE-APACHE](paste-1.0.15/LICENSE-APACHE), [LICENSE-MIT](paste-1.0.15/LICENSE-MIT) |
| pem-3.0.6 | MIT | [LICENSE.md](pem-3.0.6/LICENSE.md) |
| pem-rfc7468-0.7.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](pem-rfc7468-0.7.0/LICENSE-APACHE), [LICENSE-MIT](pem-rfc7468-0.7.0/LICENSE-MIT) |
| pem-rfc7468-1.0.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](pem-rfc7468-1.0.0/LICENSE-APACHE), [LICENSE-MIT](pem-rfc7468-1.0.0/LICENSE-MIT) |
| percent-encoding-2.3.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](percent-encoding-2.3.2/LICENSE-APACHE), [LICENSE-MIT](percent-encoding-2.3.2/LICENSE-MIT) |
| pin-project-1.1.13 | Apache-2.0 OR MIT | [LICENSE-APACHE](pin-project-1.1.13/LICENSE-APACHE), [LICENSE-MIT](pin-project-1.1.13/LICENSE-MIT) |
| pin-project-internal-1.1.13 | Apache-2.0 OR MIT | [LICENSE-APACHE](pin-project-internal-1.1.13/LICENSE-APACHE), [LICENSE-MIT](pin-project-internal-1.1.13/LICENSE-MIT) |
| pin-project-lite-0.2.17 | Apache-2.0 OR MIT | [LICENSE-APACHE](pin-project-lite-0.2.17/LICENSE-APACHE), [LICENSE-MIT](pin-project-lite-0.2.17/LICENSE-MIT) |
| pin-utils-0.1.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](pin-utils-0.1.1/LICENSE-APACHE), [LICENSE-MIT](pin-utils-0.1.1/LICENSE-MIT) |
| pkcs8-0.10.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](pkcs8-0.10.2/LICENSE-APACHE), [LICENSE-MIT](pkcs8-0.10.2/LICENSE-MIT) |
| pkcs8-0.11.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](pkcs8-0.11.0/LICENSE-APACHE), [LICENSE-MIT](pkcs8-0.11.0/LICENSE-MIT) |
| plist-1.10.3 | MIT | [LICENCE](plist-1.10.3/LICENCE) |
| poly1305-0.8.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](poly1305-0.8.0/LICENSE-APACHE), [LICENSE-MIT](poly1305-0.8.0/LICENSE-MIT) |
| polyval-0.6.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](polyval-0.6.2/LICENSE-APACHE), [LICENSE-MIT](polyval-0.6.2/LICENSE-MIT) |
| portable-atomic-1.15.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](portable-atomic-1.15.0/LICENSE-APACHE), [LICENSE-MIT](portable-atomic-1.15.0/LICENSE-MIT) |
| postcard-1.1.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](postcard-1.1.3/LICENSE-APACHE), [LICENSE-MIT](postcard-1.1.3/LICENSE-MIT) |
| postcard-derive-0.2.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](postcard-derive-0.2.2/LICENSE-APACHE), [LICENSE-MIT](postcard-derive-0.2.2/LICENSE-MIT) |
| potential_utf-0.1.6 | Unicode-3.0 | [LICENSE](potential_utf-0.1.6/LICENSE) |
| powerfmt-0.2.1 | MIT OR Apache-2.0 | [LICENSE-Apache](powerfmt-0.2.1/LICENSE-Apache), [LICENSE-MIT](powerfmt-0.2.1/LICENSE-MIT) |
| ppv-lite86-0.2.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](ppv-lite86-0.2.21/LICENSE-APACHE), [LICENSE-MIT](ppv-lite86-0.2.21/LICENSE-MIT) |
| primeorder-0.13.6 | Apache-2.0 OR MIT | [LICENSE-APACHE](primeorder-0.13.6/LICENSE-APACHE), [LICENSE-MIT](primeorder-0.13.6/LICENSE-MIT) |
| proc-macro-crate-3.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](proc-macro-crate-3.5.0/LICENSE-APACHE), [LICENSE-MIT](proc-macro-crate-3.5.0/LICENSE-MIT) |
| proc-macro2-1.0.107 | MIT OR Apache-2.0 | [LICENSE-APACHE](proc-macro2-1.0.107/LICENSE-APACHE), [LICENSE-MIT](proc-macro2-1.0.107/LICENSE-MIT) |
| quick-xml-0.42.0 | MIT | [LICENSE-MIT.md](quick-xml-0.42.0/LICENSE-MIT.md) |
| quinn-0.11.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](quinn-0.11.12/LICENSE-APACHE), [LICENSE-MIT](quinn-0.11.12/LICENSE-MIT) |
| quinn-proto-0.11.19 | MIT OR Apache-2.0 | [LICENSE-APACHE](quinn-proto-0.11.19/LICENSE-APACHE), [LICENSE-MIT](quinn-proto-0.11.19/LICENSE-MIT) |
| quinn-udp-0.5.16 | MIT OR Apache-2.0 | [LICENSE-APACHE](quinn-udp-0.5.16/LICENSE-APACHE), [LICENSE-MIT](quinn-udp-0.5.16/LICENSE-MIT) |
| quote-1.0.47 | MIT OR Apache-2.0 | [LICENSE-APACHE](quote-1.0.47/LICENSE-APACHE), [LICENSE-MIT](quote-1.0.47/LICENSE-MIT) |
| rand-0.10.3 | MIT OR Apache-2.0 | [COPYRIGHT](rand-0.10.3/COPYRIGHT), [LICENSE-APACHE](rand-0.10.3/LICENSE-APACHE), [LICENSE-MIT](rand-0.10.3/LICENSE-MIT) |
| rand-0.9.5 | MIT OR Apache-2.0 | [COPYRIGHT](rand-0.9.5/COPYRIGHT), [LICENSE-APACHE](rand-0.9.5/LICENSE-APACHE), [LICENSE-MIT](rand-0.9.5/LICENSE-MIT) |
| rand_chacha-0.9.0 | MIT OR Apache-2.0 | [COPYRIGHT](rand_chacha-0.9.0/COPYRIGHT), [LICENSE-APACHE](rand_chacha-0.9.0/LICENSE-APACHE), [LICENSE-MIT](rand_chacha-0.9.0/LICENSE-MIT) |
| rand_core-0.10.1 | MIT OR Apache-2.0 | [COPYRIGHT](rand_core-0.10.1/COPYRIGHT), [LICENSE-APACHE](rand_core-0.10.1/LICENSE-APACHE), [LICENSE-MIT](rand_core-0.10.1/LICENSE-MIT) |
| rand_core-0.6.4 | MIT OR Apache-2.0 | [COPYRIGHT](rand_core-0.6.4/COPYRIGHT), [LICENSE-APACHE](rand_core-0.6.4/LICENSE-APACHE), [LICENSE-MIT](rand_core-0.6.4/LICENSE-MIT) |
| rand_core-0.9.5 | MIT OR Apache-2.0 | [COPYRIGHT](rand_core-0.9.5/COPYRIGHT), [LICENSE-APACHE](rand_core-0.9.5/LICENSE-APACHE), [LICENSE-MIT](rand_core-0.9.5/LICENSE-MIT) |
| rand_pcg-0.10.2 | MIT OR Apache-2.0 | [COPYRIGHT](rand_pcg-0.10.2/COPYRIGHT), [LICENSE-APACHE](rand_pcg-0.10.2/LICENSE-APACHE), [LICENSE-MIT](rand_pcg-0.10.2/LICENSE-MIT) |
| rcgen-0.13.2 | MIT OR Apache-2.0 | [LICENSE](rcgen-0.13.2/LICENSE) |
| rcgen-0.14.10 | MIT OR Apache-2.0 | [LICENSE](rcgen-0.14.10/LICENSE) |
| regex-1.13.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](regex-1.13.1/LICENSE-APACHE), [LICENSE-MIT](regex-1.13.1/LICENSE-MIT) |
| regex-automata-0.4.18 | MIT OR Apache-2.0 | [LICENSE-APACHE](regex-automata-0.4.18/LICENSE-APACHE), [LICENSE-MIT](regex-automata-0.4.18/LICENSE-MIT) |
| regex-syntax-0.8.11 | MIT OR Apache-2.0 | [LICENSE-APACHE](regex-syntax-0.8.11/LICENSE-APACHE), [LICENSE-MIT](regex-syntax-0.8.11/LICENSE-MIT), [LICENSE-UNICODE](regex-syntax-0.8.11/src/unicode_tables/LICENSE-UNICODE) |
| reqwest-0.12.28 | MIT OR Apache-2.0 | [LICENSE-APACHE](reqwest-0.12.28/LICENSE-APACHE), [LICENSE-MIT](reqwest-0.12.28/LICENSE-MIT) |
| reqwest-0.13.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](reqwest-0.13.5/LICENSE-APACHE), [LICENSE-MIT](reqwest-0.13.5/LICENSE-MIT) |
| rfc6979-0.4.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](rfc6979-0.4.0/LICENSE-APACHE), [LICENSE-MIT](rfc6979-0.4.0/LICENSE-MIT) |
| ring-0.17.14 | Apache-2.0 AND ISC | [LICENSE](ring-0.17.14/LICENSE), [LICENSE-BoringSSL](ring-0.17.14/LICENSE-BoringSSL), [LICENSE-other-bits](ring-0.17.14/LICENSE-other-bits), [LICENSE-APACHE](ring-0.17.14/src/polyfill/once_cell/LICENSE-APACHE), [LICENSE-MIT](ring-0.17.14/src/polyfill/once_cell/LICENSE-MIT), [LICENSE](ring-0.17.14/third_party/fiat/LICENSE) |
| rtcp-0.14.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](rtcp-0.14.0/LICENSE-APACHE), [LICENSE-MIT](rtcp-0.14.0/LICENSE-MIT) |
| rtp-0.14.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](rtp-0.14.0/LICENSE-APACHE), [LICENSE-MIT](rtp-0.14.0/LICENSE-MIT) |
| rustc-hash-2.1.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](rustc-hash-2.1.3/LICENSE-APACHE), [LICENSE-MIT](rustc-hash-2.1.3/LICENSE-MIT) |
| rustc_version-0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](rustc_version-0.4.1/LICENSE-APACHE), [LICENSE-MIT](rustc_version-0.4.1/LICENSE-MIT) |
| rusticata-macros-4.1.0 | MIT/Apache-2.0 | [LICENSE-APACHE](rusticata-macros-4.1.0/LICENSE-APACHE), [LICENSE-MIT](rusticata-macros-4.1.0/LICENSE-MIT) |
| rustix-1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [COPYRIGHT](rustix-1.1.6/COPYRIGHT), [LICENSE-APACHE](rustix-1.1.6/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](rustix-1.1.6/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](rustix-1.1.6/LICENSE-MIT) |
| rustls-0.23.46 | Apache-2.0 OR ISC OR MIT | [LICENSE-APACHE](rustls-0.23.46/LICENSE-APACHE), [LICENSE-ISC](rustls-0.23.46/LICENSE-ISC), [LICENSE-MIT](rustls-0.23.46/LICENSE-MIT) |
| rustls-pki-types-1.15.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](rustls-pki-types-1.15.1/LICENSE-APACHE), [LICENSE-MIT](rustls-pki-types-1.15.1/LICENSE-MIT) |
| rustls-platform-verifier-0.7.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](rustls-platform-verifier-0.7.1/LICENSE-APACHE), [LICENSE-MIT](rustls-platform-verifier-0.7.1/LICENSE-MIT) |
| rustls-platform-verifier-android-0.2.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](rustls-platform-verifier-android-0.2.0/upstream-repository-root/LICENSE-APACHE), [LICENSE-MIT](rustls-platform-verifier-android-0.2.0/upstream-repository-root/LICENSE-MIT) |
| rustls-webpki-0.103.16 | ISC | [LICENSE](rustls-webpki-0.103.16/LICENSE) |
| rustversion-1.0.23 | MIT OR Apache-2.0 | [LICENSE-APACHE](rustversion-1.0.23/LICENSE-APACHE), [LICENSE-MIT](rustversion-1.0.23/LICENSE-MIT) |
| ryu-1.0.23 | Apache-2.0 OR BSL-1.0 | [LICENSE-APACHE](ryu-1.0.23/LICENSE-APACHE), [LICENSE-BOOST](ryu-1.0.23/LICENSE-BOOST) |
| same-file-1.0.6 | Unlicense/MIT | [COPYING](same-file-1.0.6/COPYING), [LICENSE-MIT](same-file-1.0.6/LICENSE-MIT), [UNLICENSE](same-file-1.0.6/UNLICENSE) |
| scopeguard-1.2.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](scopeguard-1.2.0/LICENSE-APACHE), [LICENSE-MIT](scopeguard-1.2.0/LICENSE-MIT) |
| sdp-0.10.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](sdp-0.10.0/LICENSE-APACHE), [LICENSE-MIT](sdp-0.10.0/LICENSE-MIT) |
| sec1-0.7.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](sec1-0.7.3/LICENSE-APACHE), [LICENSE-MIT](sec1-0.7.3/LICENSE-MIT) |
| security-framework-3.7.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](security-framework-3.7.0/LICENSE-APACHE), [LICENSE-MIT](security-framework-3.7.0/LICENSE-MIT) |
| security-framework-sys-2.17.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](security-framework-sys-2.17.0/LICENSE-APACHE), [LICENSE-MIT](security-framework-sys-2.17.0/LICENSE-MIT) |
| seize-0.5.1 | MIT | [LICENSE](seize-0.5.1/LICENSE) |
| semver-1.0.28 | MIT OR Apache-2.0 | [LICENSE-APACHE](semver-1.0.28/LICENSE-APACHE), [LICENSE-MIT](semver-1.0.28/LICENSE-MIT) |
| serde-1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](serde-1.0.229/LICENSE-APACHE), [LICENSE-MIT](serde-1.0.229/LICENSE-MIT) |
| serde_bytes-0.11.19 | MIT OR Apache-2.0 | [LICENSE-APACHE](serde_bytes-0.11.19/LICENSE-APACHE), [LICENSE-MIT](serde_bytes-0.11.19/LICENSE-MIT) |
| serde_core-1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](serde_core-1.0.229/LICENSE-APACHE), [LICENSE-MIT](serde_core-1.0.229/LICENSE-MIT) |
| serde_derive-1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](serde_derive-1.0.229/LICENSE-APACHE), [LICENSE-MIT](serde_derive-1.0.229/LICENSE-MIT) |
| serde_json-1.0.154 | MIT OR Apache-2.0 | [LICENSE-APACHE](serde_json-1.0.154/LICENSE-APACHE), [LICENSE-MIT](serde_json-1.0.154/LICENSE-MIT) |
| serde_path_to_error-0.1.20 | MIT OR Apache-2.0 | [LICENSE-APACHE](serde_path_to_error-0.1.20/LICENSE-APACHE), [LICENSE-MIT](serde_path_to_error-0.1.20/LICENSE-MIT) |
| serde_urlencoded-0.7.1 | MIT/Apache-2.0 | [LICENSE-APACHE](serde_urlencoded-0.7.1/LICENSE-APACHE), [LICENSE-MIT](serde_urlencoded-0.7.1/LICENSE-MIT) |
| serdect-0.4.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](serdect-0.4.3/LICENSE-APACHE), [LICENSE-MIT](serdect-0.4.3/LICENSE-MIT) |
| sha1-0.10.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](sha1-0.10.7/LICENSE-APACHE), [LICENSE-MIT](sha1-0.10.7/LICENSE-MIT) |
| sha1_smol-1.0.1 | BSD-3-Clause | [AUTHORS](sha1_smol-1.0.1/AUTHORS), [LICENSE](sha1_smol-1.0.1/LICENSE) |
| sha2-0.10.9 | MIT OR Apache-2.0 | [LICENSE-APACHE](sha2-0.10.9/LICENSE-APACHE), [LICENSE-MIT](sha2-0.10.9/LICENSE-MIT) |
| sha2-0.11.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](sha2-0.11.1/LICENSE-APACHE), [LICENSE-MIT](sha2-0.11.1/LICENSE-MIT) |
| shlex-2.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](shlex-2.0.1/LICENSE-APACHE), [LICENSE-MIT](shlex-2.0.1/LICENSE-MIT) |
| signal-hook-registry-1.4.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](signal-hook-registry-1.4.8/LICENSE-APACHE), [LICENSE-MIT](signal-hook-registry-1.4.8/LICENSE-MIT) |
| signature-2.2.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](signature-2.2.0/LICENSE-APACHE), [LICENSE-MIT](signature-2.2.0/LICENSE-MIT) |
| signature-3.0.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](signature-3.0.0/LICENSE-APACHE), [LICENSE-MIT](signature-3.0.0/LICENSE-MIT) |
| simd_cesu8-1.2.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](simd_cesu8-1.2.0/LICENSE-APACHE), [LICENSE-MIT](simd_cesu8-1.2.0/LICENSE-MIT) |
| simdutf8-0.1.5 | MIT OR Apache-2.0 | [LICENSE-Apache](simdutf8-0.1.5/LICENSE-Apache), [LICENSE-MIT](simdutf8-0.1.5/LICENSE-MIT) |
| simple-dns-0.12.0 | MIT | [LICENSE](simple-dns-0.12.0/LICENSE) |
| slab-0.4.12 | MIT | [LICENSE](slab-0.4.12/LICENSE) |
| slotmap-1.1.1 | Zlib | [LICENSE](slotmap-1.1.1/LICENSE) |
| smallvec-1.16.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](smallvec-1.16.5/LICENSE-APACHE), [LICENSE-MIT](smallvec-1.16.5/LICENSE-MIT) |
| smol_str-0.2.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](smol_str-0.2.2/LICENSE-APACHE), [LICENSE-MIT](smol_str-0.2.2/LICENSE-MIT) |
| socket2-0.5.10 | MIT OR Apache-2.0 | [LICENSE-APACHE](socket2-0.5.10/LICENSE-APACHE), [LICENSE-MIT](socket2-0.5.10/LICENSE-MIT) |
| socket2-0.6.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](socket2-0.6.5/LICENSE-APACHE), [LICENSE-MIT](socket2-0.6.5/LICENSE-MIT) |
| sorted-index-buffer-0.2.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](sorted-index-buffer-0.2.1/LICENSE-APACHE), [LICENSE-MIT](sorted-index-buffer-0.2.1/LICENSE-MIT) |
| spez-0.1.2 | BSD-2-Clause | [LICENSE](spez-0.1.2/LICENSE) |
| spin-0.10.1 | MIT | [LICENSE](spin-0.10.1/LICENSE) |
| spki-0.7.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](spki-0.7.3/LICENSE-APACHE), [LICENSE-MIT](spki-0.7.3/LICENSE-MIT) |
| spki-0.8.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](spki-0.8.1/LICENSE-APACHE), [LICENSE-MIT](spki-0.8.1/LICENSE-MIT) |
| stable_deref_trait-1.2.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](stable_deref_trait-1.2.1/LICENSE-APACHE), [LICENSE-MIT](stable_deref_trait-1.2.1/LICENSE-MIT) |
| strum-0.28.0 | MIT | [LICENSE](strum-0.28.0/LICENSE) |
| strum_macros-0.28.0 | MIT | [LICENSE](strum_macros-0.28.0/LICENSE) |
| stun-0.9.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](stun-0.9.0/LICENSE-APACHE), [LICENSE-MIT](stun-0.9.0/LICENSE-MIT) |
| substring-1.4.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](substring-1.4.5/LICENSE-APACHE), [LICENSE-MIT](substring-1.4.5/LICENSE-MIT) |
| subtle-2.6.1 | BSD-3-Clause | [LICENSE](subtle-2.6.1/LICENSE) |
| syn-2.0.119 | MIT OR Apache-2.0 | [LICENSE-APACHE](syn-2.0.119/LICENSE-APACHE), [LICENSE-MIT](syn-2.0.119/LICENSE-MIT) |
| syn-3.0.9 | MIT OR Apache-2.0 | [LICENSE-APACHE](syn-3.0.9/LICENSE-APACHE), [LICENSE-MIT](syn-3.0.9/LICENSE-MIT) |
| sync_wrapper-1.0.2 | Apache-2.0 | [LICENSE](sync_wrapper-1.0.2/LICENSE) |
| synstructure-0.13.2 | MIT | [LICENSE](synstructure-0.13.2/LICENSE) |
| synstructure-0.14.0 | MIT | [LICENSE](synstructure-0.14.0/LICENSE) |
| system-configuration-0.8.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](system-configuration-0.8.0/LICENSE-APACHE), [LICENSE-MIT](system-configuration-0.8.0/LICENSE-MIT) |
| system-configuration-sys-0.6.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](system-configuration-sys-0.6.0/LICENSE-APACHE), [LICENSE-MIT](system-configuration-sys-0.6.0/LICENSE-MIT) |
| tempfile-3.27.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](tempfile-3.27.0/LICENSE-APACHE), [LICENSE-MIT](tempfile-3.27.0/LICENSE-MIT) |
| thiserror-1.0.69 | MIT OR Apache-2.0 | [LICENSE-APACHE](thiserror-1.0.69/LICENSE-APACHE), [LICENSE-MIT](thiserror-1.0.69/LICENSE-MIT) |
| thiserror-2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](thiserror-2.0.21/LICENSE-APACHE), [LICENSE-MIT](thiserror-2.0.21/LICENSE-MIT) |
| thiserror-impl-1.0.69 | MIT OR Apache-2.0 | [LICENSE-APACHE](thiserror-impl-1.0.69/LICENSE-APACHE), [LICENSE-MIT](thiserror-impl-1.0.69/LICENSE-MIT) |
| thiserror-impl-2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](thiserror-impl-2.0.21/LICENSE-APACHE), [LICENSE-MIT](thiserror-impl-2.0.21/LICENSE-MIT) |
| time-0.3.55 | MIT OR Apache-2.0 | [LICENSE-Apache](time-0.3.55/LICENSE-Apache), [LICENSE-MIT](time-0.3.55/LICENSE-MIT) |
| time-core-0.1.9 | MIT OR Apache-2.0 | [LICENSE-Apache](time-core-0.1.9/LICENSE-Apache), [LICENSE-MIT](time-core-0.1.9/LICENSE-MIT) |
| time-macros-0.2.32 | MIT OR Apache-2.0 | [LICENSE-Apache](time-macros-0.2.32/LICENSE-Apache), [LICENSE-MIT](time-macros-0.2.32/LICENSE-MIT) |
| tinystr-0.8.5 | Unicode-3.0 | [LICENSE](tinystr-0.8.5/LICENSE) |
| tinyvec-1.13.3 | Zlib OR Apache-2.0 OR MIT | [LICENSE-APACHE.md](tinyvec-1.13.3/LICENSE-APACHE.md), [LICENSE-MIT.md](tinyvec-1.13.3/LICENSE-MIT.md), [LICENSE-ZLIB.md](tinyvec-1.13.3/LICENSE-ZLIB.md) |
| tokio-1.54.1 | MIT | [LICENSE](tokio-1.54.1/LICENSE) |
| tokio-macros-2.7.2 | MIT | [LICENSE](tokio-macros-2.7.2/LICENSE) |
| tokio-rustls-0.26.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](tokio-rustls-0.26.6/LICENSE-APACHE), [LICENSE-MIT](tokio-rustls-0.26.6/LICENSE-MIT) |
| tokio-stream-0.1.19 | MIT | [LICENSE](tokio-stream-0.1.19/LICENSE) |
| tokio-tungstenite-0.29.0 | MIT | [LICENSE](tokio-tungstenite-0.29.0/LICENSE) |
| tokio-util-0.7.20 | MIT | [LICENSE](tokio-util-0.7.20/LICENSE) |
| tokio-websockets-0.13.3 | MIT | [LICENSE](tokio-websockets-0.13.3/LICENSE) |
| toml_datetime-1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](toml_datetime-1.1.3+spec-1.1.0/LICENSE-APACHE), [LICENSE-MIT](toml_datetime-1.1.3+spec-1.1.0/LICENSE-MIT) |
| toml_edit-0.25.19+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](toml_edit-0.25.19+spec-1.1.0/LICENSE-APACHE), [LICENSE-MIT](toml_edit-0.25.19+spec-1.1.0/LICENSE-MIT) |
| toml_parser-1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](toml_parser-1.1.5+spec-1.1.0/LICENSE-APACHE), [LICENSE-MIT](toml_parser-1.1.5+spec-1.1.0/LICENSE-MIT) |
| tower-0.5.3 | MIT | [LICENSE](tower-0.5.3/LICENSE) |
| tower-http-0.6.11 | MIT | [LICENSE](tower-http-0.6.11/LICENSE) |
| tower-layer-0.3.3 | MIT | [LICENSE](tower-layer-0.3.3/LICENSE) |
| tower-service-0.3.3 | MIT | [LICENSE](tower-service-0.3.3/LICENSE) |
| tracing-0.1.44 | MIT | [LICENSE](tracing-0.1.44/LICENSE) |
| tracing-attributes-0.1.31 | MIT | [LICENSE](tracing-attributes-0.1.31/LICENSE) |
| tracing-core-0.1.36 | MIT | [LICENSE](tracing-core-0.1.36/LICENSE), [LICENSE](tracing-core-0.1.36/src/spin/LICENSE) |
| try-lock-0.2.5 | MIT | [LICENSE](try-lock-0.2.5/LICENSE) |
| tungstenite-0.29.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](tungstenite-0.29.0/LICENSE-APACHE), [LICENSE-MIT](tungstenite-0.29.0/LICENSE-MIT) |
| turn-0.11.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](turn-0.11.0/LICENSE-APACHE), [LICENSE-MIT](turn-0.11.0/LICENSE-MIT) |
| typenum-1.20.1 | MIT OR Apache-2.0 | [LICENSE](typenum-1.20.1/LICENSE), [LICENSE-APACHE](typenum-1.20.1/LICENSE-APACHE), [LICENSE-MIT](typenum-1.20.1/LICENSE-MIT) |
| unicase-2.10.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](unicase-2.10.0/LICENSE-APACHE), [LICENSE-MIT](unicase-2.10.0/LICENSE-MIT) |
| unicode-ident-1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | [LICENSE-APACHE](unicode-ident-1.0.27/LICENSE-APACHE), [LICENSE-MIT](unicode-ident-1.0.27/LICENSE-MIT), [LICENSE-UNICODE](unicode-ident-1.0.27/LICENSE-UNICODE) |
| unicode-segmentation-1.13.3 | MIT OR Apache-2.0 | [COPYRIGHT](unicode-segmentation-1.13.3/COPYRIGHT), [LICENSE-APACHE](unicode-segmentation-1.13.3/LICENSE-APACHE), [LICENSE-MIT](unicode-segmentation-1.13.3/LICENSE-MIT) |
| unicode-xid-0.2.6 | MIT OR Apache-2.0 | [COPYRIGHT](unicode-xid-0.2.6/COPYRIGHT), [LICENSE-APACHE](unicode-xid-0.2.6/LICENSE-APACHE), [LICENSE-MIT](unicode-xid-0.2.6/LICENSE-MIT) |
| universal-hash-0.5.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](universal-hash-0.5.1/LICENSE-APACHE), [LICENSE-MIT](universal-hash-0.5.1/LICENSE-MIT) |
| untrusted-0.9.0 | ISC | [LICENSE.txt](untrusted-0.9.0/LICENSE.txt) |
| url-2.5.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](url-2.5.8/LICENSE-APACHE), [LICENSE-MIT](url-2.5.8/LICENSE-MIT) |
| utf8_iter-1.1.0 | Apache-2.0 OR MIT | [COPYRIGHT](utf8_iter-1.1.0/COPYRIGHT), [LICENSE-APACHE](utf8_iter-1.1.0/LICENSE-APACHE), [LICENSE-MIT](utf8_iter-1.1.0/LICENSE-MIT) |
| utf_types-1.0.0 | Apache-2.0 OR MIT | [COPYRIGHT](utf_types-1.0.0/COPYRIGHT), [LICENSE-APACHE](utf_types-1.0.0/LICENSE-APACHE), [LICENSE-MIT](utf_types-1.0.0/LICENSE-MIT) |
| uuid-1.29.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](uuid-1.29.1/LICENSE-APACHE), [LICENSE-MIT](uuid-1.29.1/LICENSE-MIT) |
| version_check-0.9.5 | MIT/Apache-2.0 | [LICENSE-APACHE](version_check-0.9.5/LICENSE-APACHE), [LICENSE-MIT](version_check-0.9.5/LICENSE-MIT) |
| waitgroup-0.1.2 | Apache-2.0 | [LICENSE](waitgroup-0.1.2/LICENSE) |
| walkdir-2.5.0 | Unlicense/MIT | [COPYING](walkdir-2.5.0/COPYING), [LICENSE-MIT](walkdir-2.5.0/LICENSE-MIT), [UNLICENSE](walkdir-2.5.0/UNLICENSE) |
| want-0.3.2 | MIT | [LICENSE](want-0.3.2/LICENSE) |
| webpki-roots-0.26.11 | CDLA-Permissive-2.0 | [LICENSE](webpki-roots-0.26.11/LICENSE) |
| webpki-roots-1.0.11 | CDLA-Permissive-2.0 | [LICENSE](webpki-roots-1.0.11/LICENSE) |
| webrtc-0.14.0 | MIT OR Apache-2.0 | [LICENSE-MIT](webrtc-0.14.0/upstream-repository-root/LICENSE-MIT), [LICENSE-APACHE](webrtc-0.14.0/upstream-repository-root/LICENSE-APACHE) |
| webrtc-data-0.12.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](webrtc-data-0.12.0/LICENSE-APACHE), [LICENSE-MIT](webrtc-data-0.12.0/LICENSE-MIT) |
| webrtc-ice-0.14.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](webrtc-ice-0.14.0/LICENSE-APACHE), [LICENSE-MIT](webrtc-ice-0.14.0/LICENSE-MIT) |
| webrtc-mdns-0.10.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](webrtc-mdns-0.10.0/LICENSE-APACHE), [LICENSE-MIT](webrtc-mdns-0.10.0/LICENSE-MIT) |
| webrtc-media-0.11.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](webrtc-media-0.11.0/LICENSE-APACHE), [LICENSE-MIT](webrtc-media-0.11.0/LICENSE-MIT) |
| webrtc-sctp-0.13.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](webrtc-sctp-0.13.0/LICENSE-APACHE), [LICENSE-MIT](webrtc-sctp-0.13.0/LICENSE-MIT) |
| webrtc-srtp-0.16.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](webrtc-srtp-0.16.0/LICENSE-APACHE), [LICENSE-MIT](webrtc-srtp-0.16.0/LICENSE-MIT) |
| webrtc-util-0.12.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](webrtc-util-0.12.0/LICENSE-APACHE), [LICENSE-MIT](webrtc-util-0.12.0/LICENSE-MIT) |
| winnow-1.0.5 | MIT | [LICENSE-MIT](winnow-1.0.5/LICENSE-MIT) |
| writeable-0.6.4 | Unicode-3.0 | [LICENSE](writeable-0.6.4/LICENSE) |
| x25519-dalek-2.0.1 | BSD-3-Clause | [LICENSE](x25519-dalek-2.0.1/LICENSE) |
| x509-parser-0.16.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](x509-parser-0.16.0/LICENSE-APACHE), [LICENSE-MIT](x509-parser-0.16.0/LICENSE-MIT) |
| x509-parser-0.18.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](x509-parser-0.18.1/LICENSE-APACHE), [LICENSE-MIT](x509-parser-0.18.1/LICENSE-MIT) |
| yasna-0.5.2 | MIT OR Apache-2.0 | [LICENSE-MIT](yasna-0.5.2/upstream-repository-root/LICENSE-MIT), [LICENSE-APACHE](yasna-0.5.2/upstream-repository-root/LICENSE-APACHE) |
| yasna-0.6.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](yasna-0.6.0/LICENSE-APACHE), [LICENSE-MIT](yasna-0.6.0/LICENSE-MIT) |
| yoke-0.8.4 | Unicode-3.0 | [LICENSE](yoke-0.8.4/LICENSE) |
| yoke-derive-0.8.4 | Unicode-3.0 | [LICENSE](yoke-derive-0.8.4/LICENSE) |
| zerocopy-0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | [LICENSE-APACHE](zerocopy-0.8.63/LICENSE-APACHE), [LICENSE-BSD](zerocopy-0.8.63/LICENSE-BSD), [LICENSE-MIT](zerocopy-0.8.63/LICENSE-MIT) |
| zerocopy-derive-0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | [LICENSE-APACHE](zerocopy-derive-0.8.63/LICENSE-APACHE), [LICENSE-BSD](zerocopy-derive-0.8.63/LICENSE-BSD), [LICENSE-MIT](zerocopy-derive-0.8.63/LICENSE-MIT) |
| zerofrom-0.1.8 | Unicode-3.0 | [LICENSE](zerofrom-0.1.8/LICENSE) |
| zerofrom-derive-0.1.8 | Unicode-3.0 | [LICENSE](zerofrom-derive-0.1.8/LICENSE) |
| zeroize-1.9.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](zeroize-1.9.1/LICENSE-APACHE), [LICENSE-MIT](zeroize-1.9.1/LICENSE-MIT) |
| zeroize_derive-1.5.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](zeroize_derive-1.5.0/LICENSE-APACHE), [LICENSE-MIT](zeroize_derive-1.5.0/LICENSE-MIT) |
| zerotrie-0.2.5 | Unicode-3.0 | [LICENSE](zerotrie-0.2.5/LICENSE) |
| zerovec-0.11.9 | Unicode-3.0 | [LICENSE](zerovec-0.11.9/LICENSE) |
| zerovec-derive-0.11.7 | Unicode-3.0 | [LICENSE](zerovec-derive-0.11.7/LICENSE) |
| zmij-1.0.23 | MIT | [LICENSE-MIT](zmij-1.0.23/LICENSE-MIT) |
