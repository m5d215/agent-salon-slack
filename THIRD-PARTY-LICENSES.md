# Third-Party Licenses

agent-salon-slack itself is distributed under the [MIT](LICENSE) license.
The compiled binary statically links against third-party Rust code whose
own licenses must be preserved when distributing the binary.

This file enumerates those third-party components grouped by license. Full
license texts for each component are available from the upstream source
repositories and inside each crate's distribution on
[crates.io](https://crates.io).

---

## Rust crate dependencies

The following crates are compiled into the agent-salon-slack binary. Each
crate's license text is included in its source distribution on crates.io.

### Apache-2.0 (single-license)

- `rsb_derive`, `rvs_derive`, `rvstruct`, `slack-morphism`, `sync_wrapper`

### Apache-2.0 AND ISC

- `ring`

### Apache-2.0 OR BSL-1.0

- `ryu`

### Apache-2.0 OR ISC OR MIT

- `hyper-rustls`, `rustls`, `rustls-native-certs`

### Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT

- `wasi`, `wasip2`, `wasip3`, `wit-bindgen`

### BSD-2-Clause OR Apache-2.0 OR MIT

- `zerocopy`, `zerocopy-derive`

### BSD-3-Clause

- `subtle`

### CDLA-Permissive-2.0

- `webpki-roots`

### ISC

- `rustls-webpki`, `untrusted`

### MIT (single-license)

- `axum`, `axum-core`, `block2`, `bytes`
- `darling`, `darling_core`, `darling_macro`
- `data-encoding`, `generic-array`
- `h2`, `http-body`, `http-body-util`, `hyper`, `hyper-util`
- `matchers`, `matchit` (MIT AND BSD-3-Clause)
- `mime_guess`, `mio`, `nix`, `nu-ansi-term`
- `objc2`, `objc2-encode`
- `redox_syscall`, `schannel`, `sharded-slab`, `slab`, `strsim`, `synstructure`
- `tokio`, `tokio-macros`, `tokio-stream`, `tokio-tungstenite`, `tokio-util`
- `tower`, `tower-http`, `tower-layer`, `tower-service`
- `tracing`, `tracing-attributes`, `tracing-core`, `tracing-log`,
  `tracing-serde`, `tracing-subscriber`
- `try-lock`, `valuable`, `want`, `zmij`

### MIT OR Apache-2.0 (dual-license)

- `android_system_properties`, `async-recursion`, `async-trait`,
  `atomic-waker`, `base64`, `bitflags`, `block-buffer`, `bumpalo`
- `cfg-if`, `chacha20`, `chrono`, `cmov`, `const-oid`,
  `core-foundation`, `core-foundation-sys`, `cpufeatures`,
  `crypto-common`, `ctrlc`, `ctutils`
- `digest`, `displaydoc`, `equivalent`, `errno`, `fnv`,
  `form_urlencoded`, `futures`, `futures-channel`, `futures-core`,
  `futures-executor`, `futures-io`, `futures-locks`, `futures-macro`,
  `futures-sink`, `futures-task`, `futures-util`
- `getrandom`, `hashbrown`, `heck`, `hex`, `hmac`,
  `http`, `httparse`, `httpdate`, `hybrid-array`
- `iana-time-zone`, `iana-time-zone-haiku`, `ident_case`, `idna`,
  `idna_adapter`, `indexmap`, `ipnet`, `iri-string`, `itoa`,
  `js-sys`, `lazy_static`, `libc`, `lock_api`, `log`
- `mime`, `num-traits`, `once_cell`, `openssl-probe`,
  `parking_lot`, `parking_lot_core`, `percent-encoding`,
  `pin-project-lite`, `ppv-lite86`, `proc-macro2`, `quote`
- `rand`, `rand_chacha`, `rand_core`, `regex-automata`, `regex-syntax`,
  `reqwest`, `rustls-pki-types`, `scopeguard`,
  `security-framework`, `security-framework-sys`,
  `serde`, `serde_core`, `serde_derive`, `serde_json`,
  `serde_path_to_error`, `serde_urlencoded`, `serde_with`,
  `serde_with_macros`
- `sha1`, `sha2`, `signal-hook`, `signal-hook-registry`,
  `signal-hook-tokio`, `smallvec`, `socket2`, `stable_deref_trait`,
  `syn`, `thiserror`, `thiserror-impl`, `thread_local`, `tokio-rustls`,
  `tungstenite`, `typenum`, `unicase`, `url`, `utf8_iter`
- `wasm-bindgen`, `wasm-bindgen-futures`, `wasm-bindgen-macro`,
  `wasm-bindgen-macro-support`, `wasm-bindgen-shared`, `web-sys`
- `windows-core`, `windows-implement`, `windows-interface`, `windows-link`,
  `windows-result`, `windows-strings`, `windows-sys`, `windows-targets`,
  `windows_aarch64_gnullvm`, `windows_aarch64_msvc`, `windows_i686_gnu`,
  `windows_i686_gnullvm`, `windows_i686_msvc`, `windows_x86_64_gnu`,
  `windows_x86_64_gnullvm`, `windows_x86_64_msvc`, `zeroize`

### MIT OR Apache-2.0 OR LGPL-2.1-or-later

- `r-efi` — used under MIT/Apache; LGPL is not invoked.

### Unicode-3.0

- `icu_collections`, `icu_locale_core`, `icu_normalizer`,
  `icu_normalizer_data`, `icu_properties`, `icu_properties_data`,
  `icu_provider`, `litemap`, `potential_utf`, `tinystr`,
  `writeable`, `yoke`, `yoke-derive`, `zerofrom`, `zerofrom-derive`,
  `zerotrie`, `zerovec`, `zerovec-derive`

### (MIT OR Apache-2.0) AND Unicode-3.0

- `unicode-ident`

### Unlicense OR MIT

- `memchr`

### Zlib

- `foldhash`

### Zlib OR Apache-2.0 OR MIT

- `dispatch2`

---

## Apache-2.0 attribution

Per Section 4 of the Apache License 2.0, this distribution notes that it
contains code originally distributed by the upstream authors listed in
the Apache-2.0 sections above. No `NOTICE` files were identified in the
upstream crates as of this writing; if any are added upstream, they will
be mirrored here on the next update.

---

## How this file is maintained

The list above was generated from `cargo tree --prefix none --format
"{p} | {l}" --target all -e normal` against the `Cargo.lock` committed to
this repository. When dependencies change, regenerate the listing with:

```bash
cargo tree --prefix none --format "{p} | {l}" --target all -e normal | sort -u
```

and update this file accordingly.
