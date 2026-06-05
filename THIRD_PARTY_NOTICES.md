# Third-Party Notices

This project is licensed as GPL-3.0-or-later. This notice summarizes third-party material identified from the locked Rust dependency graph and bundled repository assets. It is a compliance inventory, not a legal opinion.

For binary distributions, ship this file together with `LICENSE` and the corresponding source code for this project.

## Dependency License Summary

The resolved Cargo dependency graph contains 524 third-party packages. Every resolved third-party package declares license metadata or a license file; none were missing license metadata during review.

| License expression | Package count |
| --- | ---: |
| `(Apache-2.0 OR MIT) AND BSD-3-Clause` | 1 |
| `(MIT OR Apache-2.0) AND OFL-1.1 AND LicenseRef-UFL-1.0` | 1 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |
| `0BSD OR MIT OR Apache-2.0` | 1 |
| `Apache-2.0` | 18 |
| `Apache-2.0 AND MIT` | 1 |
| `Apache-2.0 OR MIT` | 39 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 17 |
| `Apache-2.0/MIT` | 3 |
| `BSD-2-Clause` | 1 |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 2 |
| `BSD-3-Clause` | 2 |
| `BSD-3-Clause OR Apache-2.0` | 2 |
| `BSD-3-Clause OR MIT OR Apache-2.0` | 2 |
| `BSL-1.0` | 2 |
| `CC0-1.0` | 1 |
| `GPL-3.0-or-later` | 2 |
| `ISC` | 1 |
| `MIT` | 116 |
| `MIT / Apache-2.0` | 1 |
| `MIT OR Apache-2.0` | 243 |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 2 |
| `MIT OR Apache-2.0 OR Zlib` | 5 |
| `MIT OR Zlib OR Apache-2.0` | 1 |
| `MIT/Apache-2.0` | 24 |
| `Unicode-3.0` | 18 |
| `Unlicense OR MIT` | 5 |
| `Unlicense/MIT` | 2 |
| `Zlib` | 3 |
| `Zlib OR Apache-2.0 OR MIT` | 7 |

Notable items reviewed:

- `crabapple` and `crabstep` are GPL-3.0-or-later and align with this project license.
- `r-efi` is declared as `MIT OR Apache-2.0 OR LGPL-2.1-or-later`; this project can use the MIT or Apache-2.0 alternative.
- `epaint_default_fonts` includes font licenses (`OFL-1.1` and `LicenseRef-UFL-1.0`) in addition to MIT/Apache terms. Those fonts are pulled through `eframe`/`egui` for the GUI.
- Unicode ICU4X-related crates use `Unicode-3.0`; this was treated as a permissive Unicode data/code license for this inventory.

References used for license identifiers and GPL compatibility review:

- SPDX License List: <https://spdx.org/licenses/>
- GNU license compatibility notes: <https://www.gnu.org/licenses/license-list.html>

## Bundled Assets

| Path | Purpose | License/provenance note |
| --- | --- | --- |
| `docs/hero.png` | Documentation/example export image | Repository content; added by project author, no separate third-party notice found. |
| `docs/binary/img/icloud_download.png` | Documentation screenshot | Repository content; added by project author, no separate third-party notice found. |
| `docs/binary/img/safari_local_file_restrictions.png` | Documentation screenshot | Repository content; added by project author, no separate third-party notice found. |
| `imessage-exporter/src/exporters/html/resources/attachments/boar.png` | Bundled HTML example attachment | Repository content; added by project author, no separate third-party notice found. |
| `imessage-exporter/src/exporters/html/resources/attachments/davis.jpeg` | Bundled HTML example attachment | Repository content; added by project author, no separate third-party notice found. |
| `imessage-database/test_data/stickers/*.heic` | Sticker parser fixtures | Repository test fixtures; added by project author, no separate third-party notice found. |
| `imessage-database/test_data/handwritten_message/*.svg` | Handwriting renderer fixtures | Generated repository test fixtures; added by project author, no separate third-party notice found. |

## Resolved Cargo Packages

| Package | Version | License expression | Repository |
| --- | --- | --- | --- |
| `ab_glyph` | `0.2.32` | `Apache-2.0` | <https://github.com/alexheretic/ab-glyph> |
| `ab_glyph_rasterizer` | `0.1.10` | `Apache-2.0` | <https://github.com/alexheretic/ab-glyph> |
| `accesskit` | `0.16.3` | `MIT OR Apache-2.0` | <https://github.com/AccessKit/accesskit> |
| `accesskit_atspi_common` | `0.9.3` | `MIT OR Apache-2.0` | <https://github.com/AccessKit/accesskit> |
| `accesskit_consumer` | `0.24.3` | `MIT OR Apache-2.0` | <https://github.com/AccessKit/accesskit> |
| `accesskit_macos` | `0.17.4` | `MIT OR Apache-2.0` | <https://github.com/AccessKit/accesskit> |
| `accesskit_unix` | `0.12.3` | `MIT OR Apache-2.0` | <https://github.com/AccessKit/accesskit> |
| `accesskit_windows` | `0.23.2` | `MIT OR Apache-2.0` | <https://github.com/AccessKit/accesskit> |
| `accesskit_winit` | `0.22.4` | `Apache-2.0` | <https://github.com/AccessKit/accesskit> |
| `adler2` | `2.0.1` | `0BSD OR MIT OR Apache-2.0` | <https://github.com/oyvindln/adler2> |
| `aes` | `0.9.1` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/block-ciphers> |
| `aes-kw` | `0.3.1` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/key-wraps> |
| `ahash` | `0.8.12` | `MIT OR Apache-2.0` | <https://github.com/tkaitchuck/ahash> |
| `android-activity` | `0.6.1` | `MIT OR Apache-2.0` | <https://github.com/rust-mobile/android-activity> |
| `android-properties` | `0.2.2` | `MIT` | <https://github.com/miklelappo/android-properties> |
| `android_system_properties` | `0.1.5` | `MIT/Apache-2.0` | <https://github.com/nical/android_system_properties> |
| `anstream` | `1.0.0` | `MIT OR Apache-2.0` | <https://github.com/rust-cli/anstyle.git> |
| `anstyle` | `1.0.14` | `MIT OR Apache-2.0` | <https://github.com/rust-cli/anstyle.git> |
| `anstyle-parse` | `1.0.0` | `MIT OR Apache-2.0` | <https://github.com/rust-cli/anstyle.git> |
| `anstyle-query` | `1.1.5` | `MIT OR Apache-2.0` | <https://github.com/rust-cli/anstyle.git> |
| `anstyle-wincon` | `3.0.11` | `MIT OR Apache-2.0` | <https://github.com/rust-cli/anstyle.git> |
| `anyhow` | `1.0.102` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/anyhow> |
| `arboard` | `3.6.1` | `MIT OR Apache-2.0` | <https://github.com/1Password/arboard> |
| `arrayref` | `0.3.9` | `BSD-2-Clause` | <https://github.com/droundy/arrayref> |
| `arrayvec` | `0.7.6` | `MIT OR Apache-2.0` | <https://github.com/bluss/arrayvec> |
| `as-raw-xcb-connection` | `1.0.1` | `MIT OR Apache-2.0` | <https://github.com/psychon/as-raw-xcb-connection> |
| `ash` | `0.38.0+1.3.281` | `MIT OR Apache-2.0` | <https://github.com/ash-rs/ash> |
| `ashpd` | `0.11.1` | `MIT` | <https://github.com/bilelmoussaoui/ashpd> |
| `askama` | `0.16.0` | `MIT OR Apache-2.0` | <https://github.com/askama-rs/askama> |
| `askama_derive` | `0.16.0` | `MIT OR Apache-2.0` | <https://github.com/askama-rs/askama> |
| `askama_macros` | `0.16.0` | `MIT OR Apache-2.0` | <https://github.com/askama-rs/askama> |
| `askama_parser` | `0.16.0` | `MIT OR Apache-2.0` | <https://github.com/askama-rs/askama> |
| `async-broadcast` | `0.7.2` | `MIT OR Apache-2.0` | <https://github.com/smol-rs/async-broadcast> |
| `async-channel` | `2.5.0` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-channel> |
| `async-executor` | `1.14.0` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-executor> |
| `async-fs` | `2.2.0` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-fs> |
| `async-io` | `2.6.0` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-io> |
| `async-lock` | `3.4.2` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-lock> |
| `async-net` | `2.0.0` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-net> |
| `async-process` | `2.5.0` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-process> |
| `async-recursion` | `1.1.1` | `MIT OR Apache-2.0` | <https://github.com/dcchut/async-recursion> |
| `async-signal` | `0.2.14` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-signal> |
| `async-task` | `4.7.1` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/async-task> |
| `async-trait` | `0.1.89` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/async-trait> |
| `atomic-waker` | `1.1.2` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/atomic-waker> |
| `atspi` | `0.22.0` | `Apache-2.0 OR MIT` | <https://github.com/odilia-app/atspi> |
| `atspi-common` | `0.6.0` | `Apache-2.0 OR MIT` | <https://github.com/odilia-app/atspi> |
| `atspi-connection` | `0.6.0` | `Apache-2.0 OR MIT` | <https://github.com/odilia-app/atspi/> |
| `atspi-proxies` | `0.6.0` | `Apache-2.0 OR MIT` | <https://github.com/odilia-app/atspi> |
| `autocfg` | `1.5.1` | `Apache-2.0 OR MIT` | <https://github.com/cuviper/autocfg> |
| `base64` | `0.22.1` | `MIT OR Apache-2.0` | <https://github.com/marshallpierce/rust-base64> |
| `basic-toml` | `0.1.10` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/basic-toml> |
| `bit-set` | `0.6.0` | `MIT/Apache-2.0` | <https://github.com/contain-rs/bit-set> |
| `bit-vec` | `0.7.0` | `MIT/Apache-2.0` | <https://github.com/contain-rs/bit-vec> |
| `bitflags` | `1.3.2` | `MIT/Apache-2.0` | <https://github.com/bitflags/bitflags> |
| `bitflags` | `2.11.1` | `MIT OR Apache-2.0` | <https://github.com/bitflags/bitflags> |
| `block` | `0.1.6` | `MIT` | <http://github.com/SSheldon/rust-block> |
| `block-buffer` | `0.10.4` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/utils> |
| `block-buffer` | `0.12.0` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/utils> |
| `block-padding` | `0.4.2` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/utils> |
| `block2` | `0.5.1` | `MIT` | <https://github.com/madsmtm/objc2> |
| `block2` | `0.6.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `blocking` | `1.6.2` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/blocking> |
| `bstr` | `1.12.1` | `MIT OR Apache-2.0` | <https://github.com/BurntSushi/bstr> |
| `bumpalo` | `3.20.3` | `MIT OR Apache-2.0` | <https://github.com/fitzgen/bumpalo> |
| `bytemuck` | `1.25.0` | `Zlib OR Apache-2.0 OR MIT` | <https://github.com/Lokathor/bytemuck> |
| `bytemuck_derive` | `1.10.2` | `Zlib OR Apache-2.0 OR MIT` | <https://github.com/Lokathor/bytemuck> |
| `byteorder` | `1.5.0` | `Unlicense OR MIT` | <https://github.com/BurntSushi/byteorder> |
| `byteorder-lite` | `0.1.0` | `Unlicense OR MIT` | <https://github.com/image-rs/byteorder-lite> |
| `bytes` | `1.11.1` | `MIT` | <https://github.com/tokio-rs/bytes> |
| `calloop` | `0.13.0` | `MIT` | <https://github.com/Smithay/calloop> |
| `calloop` | `0.14.4` | `MIT` | <https://github.com/Smithay/calloop> |
| `calloop-wayland-source` | `0.3.0` | `MIT` | <https://github.com/smithay/calloop-wayland-source> |
| `calloop-wayland-source` | `0.4.1` | `MIT` | <https://github.com/smithay/calloop-wayland-source> |
| `cbc` | `0.2.1` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/block-modes> |
| `cc` | `1.2.63` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/cc-rs> |
| `cfg-if` | `1.0.4` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/cfg-if> |
| `cfg_aliases` | `0.1.1` | `MIT` | <https://github.com/katharostech/cfg_aliases> |
| `cfg_aliases` | `0.2.1` | `MIT` | <https://github.com/katharostech/cfg_aliases> |
| `cgl` | `0.3.2` | `MIT / Apache-2.0` | <https://github.com/servo/cgl-rs> |
| `chrono` | `0.4.44` | `MIT OR Apache-2.0` | <https://github.com/chronotope/chrono> |
| `cipher` | `0.5.2` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/traits> |
| `clap` | `4.6.1` | `MIT OR Apache-2.0` | <https://github.com/clap-rs/clap> |
| `clap_builder` | `4.6.0` | `MIT OR Apache-2.0` | <https://github.com/clap-rs/clap> |
| `clap_lex` | `1.1.0` | `MIT OR Apache-2.0` | <https://github.com/clap-rs/clap> |
| `clipboard-win` | `5.4.1` | `BSL-1.0` | <https://github.com/DoumanAsh/clipboard-win> |
| `cmov` | `0.5.4` | `Apache-2.0 OR MIT` | <https://github.com/RustCrypto/utils> |
| `codespan-reporting` | `0.11.1` | `Apache-2.0` | <https://github.com/brendanzab/codespan> |
| `color_quant` | `1.1.0` | `MIT` | <https://github.com/image-rs/color_quant.git> |
| `colorchoice` | `1.0.5` | `MIT OR Apache-2.0` | <https://github.com/rust-cli/anstyle.git> |
| `com` | `0.6.0` | `MIT` | <https://github.com/microsoft/com-rs> |
| `com_macros` | `0.6.0` | `MIT` | <https://github.com/microsoft/com-rs> |
| `com_macros_support` | `0.6.0` | `MIT` | <https://github.com/microsoft/com-rs> |
| `combine` | `4.6.7` | `MIT` | <https://github.com/Marwes/combine> |
| `concurrent-queue` | `2.5.0` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/concurrent-queue> |
| `const-oid` | `0.10.2` | `Apache-2.0 OR MIT` | <https://github.com/RustCrypto/formats> |
| `core-foundation` | `0.10.1` | `MIT OR Apache-2.0` | <https://github.com/servo/core-foundation-rs> |
| `core-foundation` | `0.9.4` | `MIT OR Apache-2.0` | <https://github.com/servo/core-foundation-rs> |
| `core-foundation-sys` | `0.8.7` | `MIT OR Apache-2.0` | <https://github.com/servo/core-foundation-rs> |
| `core-graphics` | `0.23.2` | `MIT OR Apache-2.0` | <https://github.com/servo/core-foundation-rs> |
| `core-graphics-types` | `0.1.3` | `MIT OR Apache-2.0` | <https://github.com/servo/core-foundation-rs> |
| `cpubits` | `0.1.1` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/utils> |
| `cpufeatures` | `0.2.17` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/utils> |
| `cpufeatures` | `0.3.0` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/utils> |
| `crabapple` | `0.4.7` | `GPL-3.0-or-later` | <https://github.com/ReagentX/crabapple> |
| `crabstep` | `0.4.0` | `GPL-3.0-or-later` | <https://github.com/ReagentX/crabstep> |
| `crc` | `3.4.0` | `MIT OR Apache-2.0` | <https://github.com/mrhooray/crc-rs.git> |
| `crc-catalog` | `2.5.0` | `MIT OR Apache-2.0` | <https://github.com/akhilles/crc-catalog.git> |
| `crc32fast` | `1.5.0` | `MIT OR Apache-2.0` | <https://github.com/srijs/rust-crc32fast> |
| `crossbeam-utils` | `0.8.21` | `MIT OR Apache-2.0` | <https://github.com/crossbeam-rs/crossbeam> |
| `crypto-common` | `0.1.7` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/traits> |
| `crypto-common` | `0.2.2` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/traits> |
| `ctutils` | `0.4.2` | `Apache-2.0 OR MIT` | <https://github.com/RustCrypto/utils> |
| `cursor-icon` | `1.2.0` | `MIT OR Apache-2.0 OR Zlib` | <https://github.com/rust-windowing/cursor-icon> |
| `deranged` | `0.5.8` | `MIT OR Apache-2.0` | <https://github.com/jhpratt/deranged> |
| `digest` | `0.10.7` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/traits> |
| `digest` | `0.11.3` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/traits> |
| `dispatch` | `0.2.0` | `MIT` | <http://github.com/SSheldon/rust-dispatch> |
| `dispatch2` | `0.3.1` | `Zlib OR Apache-2.0 OR MIT` | <https://github.com/madsmtm/objc2> |
| `displaydoc` | `0.2.6` | `MIT OR Apache-2.0` | <https://github.com/yaahc/displaydoc> |
| `dlib` | `0.5.3` | `MIT` | <https://github.com/elinorbgr/dlib> |
| `document-features` | `0.2.12` | `MIT OR Apache-2.0` | <https://github.com/slint-ui/document-features> |
| `downcast-rs` | `1.2.1` | `MIT/Apache-2.0` | <https://github.com/marcianx/downcast-rs> |
| `dpi` | `0.1.2` | `Apache-2.0 AND MIT` | <https://github.com/rust-windowing/winit> |
| `ecolor` | `0.29.1` | `MIT OR Apache-2.0` | <https://github.com/emilk/egui> |
| `eframe` | `0.29.1` | `MIT OR Apache-2.0` | <https://github.com/emilk/egui/tree/master/crates/eframe> |
| `egui` | `0.29.1` | `MIT OR Apache-2.0` | <https://github.com/emilk/egui> |
| `egui-wgpu` | `0.29.1` | `MIT OR Apache-2.0` | <https://github.com/emilk/egui/tree/master/crates/egui-wgpu> |
| `egui-winit` | `0.29.1` | `MIT OR Apache-2.0` | <https://github.com/emilk/egui/tree/master/crates/egui-winit> |
| `egui_glow` | `0.29.1` | `MIT OR Apache-2.0` | <https://github.com/emilk/egui/tree/master/crates/egui_glow> |
| `emath` | `0.29.1` | `MIT OR Apache-2.0` | <https://github.com/emilk/egui/tree/master/crates/emath> |
| `encoding_rs` | `0.8.35` | `(Apache-2.0 OR MIT) AND BSD-3-Clause` | <https://github.com/hsivonen/encoding_rs> |
| `endi` | `1.1.1` | `MIT` | <https://github.com/zeenix/endi> |
| `enumflags2` | `0.7.12` | `MIT OR Apache-2.0` | <https://github.com/meithecatte/enumflags2> |
| `enumflags2_derive` | `0.7.12` | `MIT OR Apache-2.0` | <https://github.com/meithecatte/enumflags2> |
| `epaint` | `0.29.1` | `MIT OR Apache-2.0` | <https://github.com/emilk/egui/tree/master/crates/epaint> |
| `epaint_default_fonts` | `0.29.1` | `(MIT OR Apache-2.0) AND OFL-1.1 AND LicenseRef-UFL-1.0` | <https://github.com/emilk/egui/tree/master/crates/epaint_default_fonts> |
| `equivalent` | `1.0.2` | `Apache-2.0 OR MIT` | <https://github.com/indexmap-rs/equivalent> |
| `errno` | `0.3.14` | `MIT OR Apache-2.0` | <https://github.com/lambda-fairy/rust-errno> |
| `error-code` | `3.3.2` | `BSL-1.0` | <https://github.com/DoumanAsh/error-code> |
| `event-listener` | `5.4.1` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/event-listener> |
| `event-listener-strategy` | `0.5.4` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/event-listener-strategy> |
| `fallible-iterator` | `0.3.0` | `MIT/Apache-2.0` | <https://github.com/sfackler/rust-fallible-iterator> |
| `fallible-streaming-iterator` | `0.1.9` | `MIT/Apache-2.0` | <https://github.com/sfackler/fallible-streaming-iterator> |
| `fastrand` | `2.4.1` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/fastrand> |
| `fdeflate` | `0.3.7` | `MIT OR Apache-2.0` | <https://github.com/image-rs/fdeflate> |
| `fdlimit` | `0.3.0` | `Apache-2.0` | <https://github.com/paritytech/fdlimit> |
| `find-msvc-tools` | `0.1.9` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/cc-rs> |
| `flate2` | `1.1.9` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/flate2-rs> |
| `foldhash` | `0.1.5` | `Zlib` | <https://github.com/orlp/foldhash> |
| `foldhash` | `0.2.0` | `Zlib` | <https://github.com/orlp/foldhash> |
| `foreign-types` | `0.5.0` | `MIT/Apache-2.0` | <https://github.com/sfackler/foreign-types> |
| `foreign-types-macros` | `0.2.3` | `MIT/Apache-2.0` | <https://github.com/sfackler/foreign-types> |
| `foreign-types-shared` | `0.3.1` | `MIT/Apache-2.0` | <https://github.com/sfackler/foreign-types> |
| `form_urlencoded` | `1.2.2` | `MIT OR Apache-2.0` | <https://github.com/servo/rust-url> |
| `fs2` | `0.4.3` | `MIT/Apache-2.0` | <https://github.com/danburkert/fs2-rs> |
| `futures-channel` | `0.3.32` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/futures-rs> |
| `futures-core` | `0.3.32` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/futures-rs> |
| `futures-io` | `0.3.32` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/futures-rs> |
| `futures-lite` | `2.6.1` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/futures-lite> |
| `futures-macro` | `0.3.32` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/futures-rs> |
| `futures-sink` | `0.3.32` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/futures-rs> |
| `futures-task` | `0.3.32` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/futures-rs> |
| `futures-util` | `0.3.32` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/futures-rs> |
| `generic-array` | `0.14.7` | `MIT` | <https://github.com/fizyk20/generic-array.git> |
| `gethostname` | `1.1.0` | `Apache-2.0` | <https://codeberg.org/swsnr/gethostname.rs.git> |
| `getrandom` | `0.2.17` | `MIT OR Apache-2.0` | <https://github.com/rust-random/getrandom> |
| `getrandom` | `0.3.4` | `MIT OR Apache-2.0` | <https://github.com/rust-random/getrandom> |
| `getrandom` | `0.4.2` | `MIT OR Apache-2.0` | <https://github.com/rust-random/getrandom> |
| `gif` | `0.13.3` | `MIT OR Apache-2.0` | <https://github.com/image-rs/image-gif> |
| `gl_generator` | `0.14.0` | `Apache-2.0` | <https://github.com/brendanzab/gl-rs/> |
| `glob` | `0.3.3` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/glob> |
| `glow` | `0.13.1` | `MIT OR Apache-2.0 OR Zlib` | <https://github.com/grovesNL/glow> |
| `glow` | `0.14.2` | `MIT OR Apache-2.0 OR Zlib` | <https://github.com/grovesNL/glow> |
| `glutin` | `0.32.3` | `Apache-2.0` | <https://github.com/rust-windowing/glutin> |
| `glutin-winit` | `0.5.0` | `MIT` | <https://github.com/rust-windowing/glutin> |
| `glutin_egl_sys` | `0.7.1` | `Apache-2.0` | <https://github.com/rust-windowing/glutin> |
| `glutin_glx_sys` | `0.6.1` | `Apache-2.0` | <https://github.com/rust-windowing/glutin> |
| `glutin_wgl_sys` | `0.6.1` | `Apache-2.0` | <https://github.com/rust-windowing/glutin> |
| `gpu-alloc` | `0.6.0` | `MIT OR Apache-2.0` | <https://github.com/zakarumych/gpu-alloc> |
| `gpu-alloc-types` | `0.3.0` | `MIT OR Apache-2.0` | <https://github.com/zakarumych/gpu-alloc> |
| `gpu-allocator` | `0.26.0` | `MIT OR Apache-2.0` | <https://github.com/Traverse-Research/gpu-allocator> |
| `gpu-descriptor` | `0.3.2` | `MIT OR Apache-2.0` | <https://github.com/zakarumych/gpu-descriptor> |
| `gpu-descriptor-types` | `0.2.0` | `MIT OR Apache-2.0` | <https://github.com/zakarumych/gpu-descriptor> |
| `hashbrown` | `0.15.5` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/hashbrown> |
| `hashbrown` | `0.16.1` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/hashbrown> |
| `hashbrown` | `0.17.1` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/hashbrown> |
| `hashlink` | `0.11.0` | `MIT OR Apache-2.0` | <https://github.com/kyren/hashlink> |
| `hassle-rs` | `0.11.0` | `MIT` | <https://github.com/Traverse-Research/hassle-rs> |
| `heck` | `0.5.0` | `MIT OR Apache-2.0` | <https://github.com/withoutboats/heck> |
| `hermit-abi` | `0.5.2` | `MIT OR Apache-2.0` | <https://github.com/hermit-os/hermit-rs> |
| `hex` | `0.4.3` | `MIT OR Apache-2.0` | <https://github.com/KokaKiwi/rust-hex> |
| `hexf-parse` | `0.2.1` | `CC0-1.0` | <https://github.com/lifthrasiir/hexf> |
| `hmac` | `0.13.0` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/MACs> |
| `hybrid-array` | `0.4.12` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/hybrid-array> |
| `iana-time-zone` | `0.1.65` | `MIT OR Apache-2.0` | <https://github.com/strawlab/iana-time-zone> |
| `iana-time-zone-haiku` | `0.1.2` | `MIT OR Apache-2.0` | <https://github.com/strawlab/iana-time-zone> |
| `icu_collections` | `2.2.0` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `icu_locale_core` | `2.2.0` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `icu_normalizer` | `2.2.0` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `icu_normalizer_data` | `2.2.0` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `icu_properties` | `2.2.0` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `icu_properties_data` | `2.2.0` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `icu_provider` | `2.2.0` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `id-arena` | `2.3.0` | `MIT/Apache-2.0` | <https://github.com/fitzgen/id-arena> |
| `idna` | `1.1.0` | `MIT OR Apache-2.0` | <https://github.com/servo/rust-url/> |
| `idna_adapter` | `1.2.2` | `Apache-2.0 OR MIT` | <https://github.com/hsivonen/idna_adapter> |
| `image` | `0.24.9` | `MIT OR Apache-2.0` | <https://github.com/image-rs/image> |
| `image` | `0.25.10` | `MIT OR Apache-2.0` | <https://github.com/image-rs/image> |
| `immutable-chunkmap` | `2.1.2` | `Apache-2.0 OR MIT` | <https://github.com/estokes/immutable-chunkmap> |
| `indexmap` | `2.14.0` | `Apache-2.0 OR MIT` | <https://github.com/indexmap-rs/indexmap> |
| `inout` | `0.2.2` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/utils> |
| `is-docker` | `0.2.0` | `MIT` | <https://github.com/TheLarkInn/is-docker> |
| `is-wsl` | `0.4.0` | `MIT` | <https://github.com/TheLarkInn/is-wsl> |
| `is_terminal_polyfill` | `1.70.2` | `MIT OR Apache-2.0` | <https://github.com/polyfill-rs/is_terminal_polyfill> |
| `itoa` | `1.0.18` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/itoa> |
| `jni` | `0.22.4` | `MIT OR Apache-2.0` | <https://github.com/jni-rs/jni-rs> |
| `jni-macros` | `0.22.4` | `MIT OR Apache-2.0` | <https://github.com/jni-rs/jni-rs> |
| `jni-sys` | `0.3.1` | `MIT OR Apache-2.0` | <https://github.com/jni-rs/jni-sys> |
| `jni-sys` | `0.4.1` | `MIT OR Apache-2.0` | <https://github.com/jni-rs/jni-sys> |
| `jni-sys-macros` | `0.4.1` | `MIT OR Apache-2.0` | <https://github.com/jni-rs/jni-sys> |
| `jobserver` | `0.1.34` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/jobserver-rs> |
| `jpeg-decoder` | `0.3.2` | `MIT OR Apache-2.0` | <https://github.com/image-rs/jpeg-decoder> |
| `js-sys` | `0.3.99` | `MIT OR Apache-2.0` | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys> |
| `jzon` | `0.12.5` | `MIT/Apache-2.0` | <https://github.com/rustadopt/jzon-rs> |
| `khronos-egl` | `6.0.0` | `MIT/Apache-2.0` | <https://github.com/timothee-haudebourg/khronos-egl> |
| `khronos_api` | `3.1.0` | `Apache-2.0` | <https://github.com/brendanzab/gl-rs/> |
| `leb128fmt` | `0.1.0` | `MIT OR Apache-2.0` | <https://github.com/bluk/leb128fmt> |
| `libc` | `0.2.186` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/libc> |
| `libloading` | `0.8.9` | `ISC` | <https://github.com/nagisa/rust_libloading/> |
| `libredox` | `0.1.17` | `MIT` | <https://gitlab.redox-os.org/redox-os/libredox.git> |
| `libsqlite3-sys` | `0.38.0` | `MIT` | <https://github.com/rusqlite/rusqlite> |
| `linked-hash-map` | `0.5.6` | `MIT/Apache-2.0` | <https://github.com/contain-rs/linked-hash-map> |
| `linux-raw-sys` | `0.12.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/sunfishcode/linux-raw-sys> |
| `linux-raw-sys` | `0.4.15` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/sunfishcode/linux-raw-sys> |
| `litemap` | `0.8.2` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `litrs` | `1.0.0` | `MIT OR Apache-2.0` | <https://github.com/LukasKalbertodt/litrs> |
| `lock_api` | `0.4.14` | `MIT OR Apache-2.0` | <https://github.com/Amanieu/parking_lot> |
| `log` | `0.4.30` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/log> |
| `lopdf` | `0.31.0` | `MIT` | <https://github.com/J-F-Liu/lopdf.git> |
| `lzma-rs` | `0.3.0` | `MIT` | <https://github.com/gendx/lzma-rs> |
| `malloc_buf` | `0.0.6` | `MIT` | <https://github.com/SSheldon/malloc_buf> |
| `md5` | `0.7.0` | `Apache-2.0/MIT` | <https://github.com/stainless-steel/md5> |
| `memchr` | `2.8.1` | `Unlicense OR MIT` | <https://github.com/BurntSushi/memchr> |
| `memmap2` | `0.9.10` | `MIT OR Apache-2.0` | <https://github.com/RazrFalcon/memmap2-rs> |
| `memoffset` | `0.9.1` | `MIT` | <https://github.com/Gilnaa/memoffset> |
| `metal` | `0.29.0` | `MIT OR Apache-2.0` | <https://github.com/gfx-rs/metal-rs> |
| `miniz_oxide` | `0.8.9` | `MIT OR Zlib OR Apache-2.0` | <https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide> |
| `moxcms` | `0.8.1` | `BSD-3-Clause OR Apache-2.0` | <https://github.com/awxkee/moxcms.git> |
| `naga` | `22.1.0` | `MIT OR Apache-2.0` | <https://github.com/gfx-rs/wgpu/tree/trunk/naga> |
| `ndk` | `0.9.0` | `MIT OR Apache-2.0` | <https://github.com/rust-mobile/ndk> |
| `ndk-context` | `0.1.1` | `MIT OR Apache-2.0` | <https://github.com/rust-windowing/android-ndk-rs> |
| `ndk-sys` | `0.5.0+25.2.9519653` | `MIT OR Apache-2.0` | <https://github.com/rust-mobile/ndk> |
| `ndk-sys` | `0.6.0+11769913` | `MIT OR Apache-2.0` | <https://github.com/rust-mobile/ndk> |
| `nix` | `0.29.0` | `MIT` | <https://github.com/nix-rust/nix> |
| `nohash-hasher` | `0.2.0` | `Apache-2.0 OR MIT` | <https://github.com/paritytech/nohash-hasher> |
| `num-conv` | `0.2.2` | `MIT OR Apache-2.0` | <https://github.com/jhpratt/num-conv> |
| `num-traits` | `0.2.19` | `MIT OR Apache-2.0` | <https://github.com/rust-num/num-traits> |
| `num_enum` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | <https://github.com/illicitonion/num_enum> |
| `num_enum_derive` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | <https://github.com/illicitonion/num_enum> |
| `objc` | `0.2.7` | `MIT` | <http://github.com/SSheldon/rust-objc> |
| `objc-sys` | `0.3.5` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2` | `0.5.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2` | `0.6.4` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-app-kit` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-app-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-cloud-kit` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-contacts` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-core-data` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-core-foundation` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-core-graphics` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-core-image` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-core-location` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-encode` | `4.1.0` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-foundation` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-foundation` | `0.3.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-io-surface` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-link-presentation` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-metal` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-quartz-core` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-symbols` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-ui-kit` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-uniform-type-identifiers` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `objc2-user-notifications` | `0.2.2` | `MIT` | <https://github.com/madsmtm/objc2> |
| `once_cell` | `1.21.4` | `MIT OR Apache-2.0` | <https://github.com/matklad/once_cell> |
| `once_cell_polyfill` | `1.70.2` | `MIT OR Apache-2.0` | <https://github.com/polyfill-rs/once_cell_polyfill> |
| `open` | `5.3.5` | `MIT` | <https://github.com/Byron/open-rs> |
| `orbclient` | `0.3.55` | `MIT` | <https://gitlab.redox-os.org/redox-os/orbclient> |
| `ordered-stream` | `0.2.0` | `MIT OR Apache-2.0` | <https://github.com/danieldg/ordered-stream> |
| `owned_ttf_parser` | `0.19.0` | `Apache-2.0` | <https://github.com/alexheretic/owned-ttf-parser> |
| `owned_ttf_parser` | `0.25.1` | `Apache-2.0` | <https://github.com/alexheretic/owned-ttf-parser> |
| `parking` | `2.2.1` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/parking> |
| `parking_lot` | `0.12.5` | `MIT OR Apache-2.0` | <https://github.com/Amanieu/parking_lot> |
| `parking_lot_core` | `0.9.12` | `MIT OR Apache-2.0` | <https://github.com/Amanieu/parking_lot> |
| `paste` | `1.0.15` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/paste> |
| `pathdiff` | `0.2.3` | `MIT/Apache-2.0` | <https://github.com/Manishearth/pathdiff> |
| `pbkdf2` | `0.13.0` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/password-hashes> |
| `percent-encoding` | `2.3.2` | `MIT OR Apache-2.0` | <https://github.com/servo/rust-url/> |
| `pin-project` | `1.1.13` | `Apache-2.0 OR MIT` | <https://github.com/taiki-e/pin-project> |
| `pin-project-internal` | `1.1.13` | `Apache-2.0 OR MIT` | <https://github.com/taiki-e/pin-project> |
| `pin-project-lite` | `0.2.17` | `Apache-2.0 OR MIT` | <https://github.com/taiki-e/pin-project-lite> |
| `piper` | `0.2.5` | `MIT OR Apache-2.0` | <https://github.com/smol-rs/piper> |
| `pkg-config` | `0.3.33` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/pkg-config-rs> |
| `plain` | `0.2.3` | `MIT/Apache-2.0` | <https://github.com/randomites/plain> |
| `plist` | `1.9.0` | `MIT` | <https://github.com/ebarnard/rust-plist/> |
| `png` | `0.17.16` | `MIT OR Apache-2.0` | <https://github.com/image-rs/image-png> |
| `png` | `0.18.1` | `MIT OR Apache-2.0` | <https://github.com/image-rs/image-png> |
| `polling` | `3.11.0` | `Apache-2.0 OR MIT` | <https://github.com/smol-rs/polling> |
| `pollster` | `0.4.0` | `Apache-2.0/MIT` | <https://github.com/zesterer/pollster> |
| `pom` | `3.4.0` | `MIT` | <https://github.com/J-F-Liu/pom.git> |
| `potential_utf` | `0.1.5` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `powerfmt` | `0.2.0` | `MIT OR Apache-2.0` | <https://github.com/jhpratt/powerfmt> |
| `ppv-lite86` | `0.2.21` | `MIT OR Apache-2.0` | <https://github.com/cryptocorrosion/cryptocorrosion> |
| `presser` | `0.3.1` | `MIT OR Apache-2.0` | <https://github.com/EmbarkStudios/presser> |
| `prettyplease` | `0.2.37` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/prettyplease> |
| `printpdf` | `0.7.0` | `MIT` | <https://github.com/fschutt/printpdf> |
| `proc-macro-crate` | `3.5.0` | `MIT OR Apache-2.0` | <https://github.com/bkchr/proc-macro-crate> |
| `proc-macro2` | `1.0.106` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/proc-macro2> |
| `profiling` | `1.0.18` | `MIT OR Apache-2.0` | <https://github.com/aclysma/profiling> |
| `protobuf` | `3.7.2` | `MIT` | <https://github.com/stepancheg/rust-protobuf/> |
| `protobuf-support` | `3.7.2` | `MIT` | <https://github.com/stepancheg/rust-protobuf/> |
| `pxfm` | `0.1.29` | `BSD-3-Clause OR Apache-2.0` | <https://github.com/awxkee/pxfm> |
| `quick-xml` | `0.30.0` | `MIT` | <https://github.com/tafia/quick-xml> |
| `quick-xml` | `0.39.4` | `MIT` | <https://github.com/tafia/quick-xml> |
| `quote` | `1.0.45` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/quote> |
| `r-efi` | `5.3.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | <https://github.com/r-efi/r-efi> |
| `r-efi` | `6.0.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | <https://github.com/r-efi/r-efi> |
| `rand` | `0.8.6` | `MIT OR Apache-2.0` | <https://github.com/rust-random/rand> |
| `rand` | `0.9.4` | `MIT OR Apache-2.0` | <https://github.com/rust-random/rand> |
| `rand_chacha` | `0.3.1` | `MIT OR Apache-2.0` | <https://github.com/rust-random/rand> |
| `rand_chacha` | `0.9.0` | `MIT OR Apache-2.0` | <https://github.com/rust-random/rand> |
| `rand_core` | `0.6.4` | `MIT OR Apache-2.0` | <https://github.com/rust-random/rand> |
| `rand_core` | `0.9.5` | `MIT OR Apache-2.0` | <https://github.com/rust-random/rand> |
| `raw-window-handle` | `0.6.2` | `MIT OR Apache-2.0 OR Zlib` | <https://github.com/rust-windowing/raw-window-handle> |
| `redox_syscall` | `0.4.1` | `MIT` | <https://gitlab.redox-os.org/redox-os/syscall> |
| `redox_syscall` | `0.5.18` | `MIT` | <https://gitlab.redox-os.org/redox-os/syscall> |
| `redox_syscall` | `0.8.1` | `MIT` | <https://gitlab.redox-os.org/redox-os/syscall> |
| `regex-automata` | `0.4.14` | `MIT OR Apache-2.0` | <https://github.com/rust-lang/regex> |
| `renderdoc-sys` | `1.1.0` | `MIT OR Apache-2.0` | <https://github.com/ebkalderon/renderdoc-rs> |
| `rfd` | `0.15.4` | `MIT` | <https://github.com/PolyMeilex/rfd> |
| `rpassword` | `7.5.3` | `Apache-2.0` | <https://github.com/conradkleinespel/rpassword> |
| `rsqlite-vfs` | `0.1.1` | `MIT` |  |
| `rtoolbox` | `0.0.5` | `Apache-2.0` | <https://github.com/conradkleinespel/rtoolbox> |
| `rusqlite` | `0.40.0` | `MIT` | <https://github.com/rusqlite/rusqlite> |
| `rustc-hash` | `1.1.0` | `Apache-2.0/MIT` | <https://github.com/rust-lang-nursery/rustc-hash> |
| `rustc-hash` | `2.1.2` | `Apache-2.0 OR MIT` | <https://github.com/rust-lang/rustc-hash> |
| `rustc_version` | `0.4.1` | `MIT OR Apache-2.0` | <https://github.com/djc/rustc-version-rs> |
| `rustix` | `0.38.44` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/rustix> |
| `rustix` | `1.1.4` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/rustix> |
| `rustversion` | `1.0.22` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/rustversion> |
| `same-file` | `1.0.6` | `Unlicense/MIT` | <https://github.com/BurntSushi/same-file> |
| `scoped-tls` | `1.0.1` | `MIT/Apache-2.0` | <https://github.com/alexcrichton/scoped-tls> |
| `scopeguard` | `1.2.0` | `MIT OR Apache-2.0` | <https://github.com/bluss/scopeguard> |
| `sctk-adwaita` | `0.10.1` | `MIT` | <https://github.com/PolyMeilex/sctk-adwaita> |
| `semver` | `1.0.28` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/semver> |
| `serde` | `1.0.228` | `MIT OR Apache-2.0` | <https://github.com/serde-rs/serde> |
| `serde_core` | `1.0.228` | `MIT OR Apache-2.0` | <https://github.com/serde-rs/serde> |
| `serde_derive` | `1.0.228` | `MIT OR Apache-2.0` | <https://github.com/serde-rs/serde> |
| `serde_json` | `1.0.150` | `MIT OR Apache-2.0` | <https://github.com/serde-rs/json> |
| `serde_repr` | `0.1.20` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/serde-repr> |
| `sha1` | `0.10.6` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/hashes> |
| `sha1` | `0.11.0` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/hashes> |
| `sha2` | `0.11.0` | `MIT OR Apache-2.0` | <https://github.com/RustCrypto/hashes> |
| `shlex` | `2.0.1` | `MIT OR Apache-2.0` | <https://github.com/comex/rust-shlex> |
| `signal-hook-registry` | `1.4.8` | `MIT OR Apache-2.0` | <https://github.com/vorner/signal-hook> |
| `simd-adler32` | `0.3.9` | `MIT` | <https://github.com/mcountryman/simd-adler32> |
| `simd_cesu8` | `1.1.1` | `Apache-2.0 OR MIT` | <https://github.com/seancroach/simd_cesu8> |
| `simdutf8` | `0.1.5` | `MIT OR Apache-2.0` | <https://github.com/rusticstuff/simdutf8> |
| `slab` | `0.4.12` | `MIT` | <https://github.com/tokio-rs/slab> |
| `slotmap` | `1.1.1` | `Zlib` | <https://github.com/orlp/slotmap> |
| `smallvec` | `1.15.1` | `MIT OR Apache-2.0` | <https://github.com/servo/rust-smallvec> |
| `smithay-client-toolkit` | `0.19.2` | `MIT` | <https://github.com/smithay/client-toolkit> |
| `smithay-client-toolkit` | `0.20.0` | `MIT` | <https://github.com/smithay/client-toolkit> |
| `smithay-clipboard` | `0.7.3` | `MIT` | <https://github.com/smithay/smithay-clipboard> |
| `smol_str` | `0.2.2` | `MIT OR Apache-2.0` | <https://github.com/rust-analyzer/smol_str> |
| `spirv` | `0.3.0+sdk-1.3.268.0` | `Apache-2.0` | <https://github.com/gfx-rs/rspirv> |
| `sqlite-wasm-rs` | `0.5.5` | `MIT` | <https://github.com/Spxg/sqlite-wasm-rs> |
| `stable_deref_trait` | `1.2.1` | `MIT OR Apache-2.0` | <https://github.com/storyyeller/stable_deref_trait> |
| `static_assertions` | `1.1.0` | `MIT OR Apache-2.0` | <https://github.com/nvzqz/static-assertions-rs> |
| `strict-num` | `0.1.1` | `MIT` | <https://github.com/RazrFalcon/strict-num> |
| `strsim` | `0.11.1` | `MIT` | <https://github.com/rapidfuzz/strsim-rs> |
| `syn` | `1.0.109` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/syn> |
| `syn` | `2.0.117` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/syn> |
| `synstructure` | `0.13.2` | `MIT` | <https://github.com/mystor/synstructure> |
| `tempfile` | `3.27.0` | `MIT OR Apache-2.0` | <https://github.com/Stebalien/tempfile> |
| `termcolor` | `1.4.1` | `Unlicense OR MIT` | <https://github.com/BurntSushi/termcolor> |
| `thiserror` | `1.0.69` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/thiserror> |
| `thiserror` | `2.0.18` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/thiserror> |
| `thiserror-impl` | `1.0.69` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/thiserror> |
| `thiserror-impl` | `2.0.18` | `MIT OR Apache-2.0` | <https://github.com/dtolnay/thiserror> |
| `tiff` | `0.9.1` | `MIT` | <https://github.com/image-rs/image-tiff> |
| `time` | `0.3.47` | `MIT OR Apache-2.0` | <https://github.com/time-rs/time> |
| `time-core` | `0.1.8` | `MIT OR Apache-2.0` | <https://github.com/time-rs/time> |
| `time-macros` | `0.2.27` | `MIT OR Apache-2.0` | <https://github.com/time-rs/time> |
| `tiny-skia` | `0.11.4` | `BSD-3-Clause` | <https://github.com/RazrFalcon/tiny-skia> |
| `tiny-skia-path` | `0.11.4` | `BSD-3-Clause` | <https://github.com/RazrFalcon/tiny-skia/tree/master/path> |
| `tinystr` | `0.8.3` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `toml_datetime` | `1.1.1+spec-1.1.0` | `MIT OR Apache-2.0` | <https://github.com/toml-rs/toml> |
| `toml_edit` | `0.25.12+spec-1.1.0` | `MIT OR Apache-2.0` | <https://github.com/toml-rs/toml> |
| `toml_parser` | `1.1.2+spec-1.1.0` | `MIT OR Apache-2.0` | <https://github.com/toml-rs/toml> |
| `tracing` | `0.1.44` | `MIT` | <https://github.com/tokio-rs/tracing> |
| `tracing-attributes` | `0.1.31` | `MIT` | <https://github.com/tokio-rs/tracing> |
| `tracing-core` | `0.1.36` | `MIT` | <https://github.com/tokio-rs/tracing> |
| `ttf-parser` | `0.19.2` | `MIT OR Apache-2.0` | <https://github.com/RazrFalcon/ttf-parser> |
| `ttf-parser` | `0.25.1` | `MIT OR Apache-2.0` | <https://github.com/harfbuzz/ttf-parser> |
| `type-map` | `0.5.1` | `MIT/Apache-2.0` | <https://github.com/kardeiz/type-map> |
| `typenum` | `1.20.1` | `MIT OR Apache-2.0` | <https://github.com/paholg/typenum> |
| `uds_windows` | `1.2.1` | `MIT` | <https://github.com/haraldh/rust_uds_windows> |
| `unicode-ident` | `1.0.24` | `(MIT OR Apache-2.0) AND Unicode-3.0` | <https://github.com/dtolnay/unicode-ident> |
| `unicode-segmentation` | `1.13.3` | `MIT OR Apache-2.0` | <https://github.com/unicode-rs/unicode-segmentation> |
| `unicode-width` | `0.1.14` | `MIT OR Apache-2.0` | <https://github.com/unicode-rs/unicode-width> |
| `unicode-xid` | `0.2.6` | `MIT OR Apache-2.0` | <https://github.com/unicode-rs/unicode-xid> |
| `url` | `2.5.8` | `MIT OR Apache-2.0` | <https://github.com/servo/rust-url> |
| `urlencoding` | `2.1.3` | `MIT` | <https://github.com/kornelski/rust_urlencoding> |
| `utf8_iter` | `1.0.4` | `Apache-2.0 OR MIT` | <https://github.com/hsivonen/utf8_iter> |
| `utf8parse` | `0.2.2` | `Apache-2.0 OR MIT` | <https://github.com/alacritty/vte> |
| `uuid` | `1.23.2` | `Apache-2.0 OR MIT` | <https://github.com/uuid-rs/uuid> |
| `vcpkg` | `0.2.15` | `MIT/Apache-2.0` | <https://github.com/mcgoo/vcpkg-rs> |
| `version_check` | `0.9.5` | `MIT/Apache-2.0` | <https://github.com/SergioBenitez/version_check> |
| `walkdir` | `2.5.0` | `Unlicense/MIT` | <https://github.com/BurntSushi/walkdir> |
| `wasi` | `0.11.1+wasi-snapshot-preview1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wasi> |
| `wasip2` | `1.0.3+wasi-0.2.9` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wasi-rs> |
| `wasip3` | `0.4.0+wasi-0.3.0-rc-2026-01-06` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wasi-rs> |
| `wasm-bindgen` | `0.2.122` | `MIT OR Apache-2.0` | <https://github.com/wasm-bindgen/wasm-bindgen> |
| `wasm-bindgen-futures` | `0.4.72` | `MIT OR Apache-2.0` | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures> |
| `wasm-bindgen-macro` | `0.2.122` | `MIT OR Apache-2.0` | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro> |
| `wasm-bindgen-macro-support` | `0.2.122` | `MIT OR Apache-2.0` | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro-support> |
| `wasm-bindgen-shared` | `0.2.122` | `MIT OR Apache-2.0` | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared> |
| `wasm-encoder` | `0.244.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wasm-encoder> |
| `wasm-metadata` | `0.244.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wasm-metadata> |
| `wasmparser` | `0.244.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wasmparser> |
| `wayland-backend` | `0.3.15` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-client` | `0.31.14` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-csd-frame` | `0.3.0` | `MIT` | <https://github.com/rust-windowing/wayland-csd-frame> |
| `wayland-cursor` | `0.31.14` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-protocols` | `0.32.12` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-protocols-experimental` | `20250721.0.1` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-protocols-misc` | `0.3.12` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-protocols-plasma` | `0.3.12` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-protocols-wlr` | `0.3.12` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-scanner` | `0.31.10` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `wayland-sys` | `0.31.11` | `MIT` | <https://github.com/smithay/wayland-rs> |
| `web-sys` | `0.3.99` | `MIT OR Apache-2.0` | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys> |
| `web-time` | `1.1.0` | `MIT OR Apache-2.0` | <https://github.com/daxpedda/web-time> |
| `webbrowser` | `1.2.1` | `MIT OR Apache-2.0` | <https://github.com/amodm/webbrowser-rs> |
| `weezl` | `0.1.12` | `MIT OR Apache-2.0` | <https://github.com/image-rs/weezl> |
| `wgpu` | `22.1.0` | `MIT OR Apache-2.0` | <https://github.com/gfx-rs/wgpu> |
| `wgpu-core` | `22.1.0` | `MIT OR Apache-2.0` | <https://github.com/gfx-rs/wgpu> |
| `wgpu-hal` | `22.0.0` | `MIT OR Apache-2.0` | <https://github.com/gfx-rs/wgpu> |
| `wgpu-types` | `22.0.0` | `MIT OR Apache-2.0` | <https://github.com/gfx-rs/wgpu> |
| `widestring` | `1.2.1` | `MIT OR Apache-2.0` | <https://github.com/VoidStarKat/widestring-rs> |
| `winapi` | `0.3.9` | `MIT/Apache-2.0` | <https://github.com/retep998/winapi-rs> |
| `winapi-i686-pc-windows-gnu` | `0.4.0` | `MIT/Apache-2.0` | <https://github.com/retep998/winapi-rs> |
| `winapi-util` | `0.1.11` | `Unlicense OR MIT` | <https://github.com/BurntSushi/winapi-util> |
| `winapi-x86_64-pc-windows-gnu` | `0.4.0` | `MIT/Apache-2.0` | <https://github.com/retep998/winapi-rs> |
| `windows` | `0.52.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows` | `0.58.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-core` | `0.52.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-core` | `0.58.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-core` | `0.62.2` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-implement` | `0.58.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-implement` | `0.60.2` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-interface` | `0.58.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-interface` | `0.59.3` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-link` | `0.2.1` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-result` | `0.2.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-result` | `0.4.1` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-strings` | `0.1.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-strings` | `0.5.1` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-sys` | `0.52.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-sys` | `0.59.0` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-sys` | `0.61.2` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows-targets` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows_aarch64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows_aarch64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows_i686_gnu` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows_i686_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows_i686_msvc` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_gnu` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `windows_x86_64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | <https://github.com/microsoft/windows-rs> |
| `winit` | `0.30.13` | `Apache-2.0` | <https://github.com/rust-windowing/winit> |
| `winnow` | `1.0.3` | `MIT` | <https://github.com/winnow-rs/winnow> |
| `wit-bindgen` | `0.51.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wit-bindgen> |
| `wit-bindgen` | `0.57.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wit-bindgen> |
| `wit-bindgen-core` | `0.51.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wit-bindgen> |
| `wit-bindgen-rust` | `0.51.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wit-bindgen> |
| `wit-bindgen-rust-macro` | `0.51.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wit-bindgen> |
| `wit-component` | `0.244.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wit-component> |
| `wit-parser` | `0.244.0` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | <https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wit-parser> |
| `writeable` | `0.6.3` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `x11-dl` | `2.21.0` | `MIT` | <https://github.com/AltF02/x11-rs.git> |
| `x11rb` | `0.13.2` | `MIT OR Apache-2.0` | <https://github.com/psychon/x11rb> |
| `x11rb-protocol` | `0.13.2` | `MIT OR Apache-2.0` | <https://github.com/psychon/x11rb> |
| `xcursor` | `0.3.10` | `MIT` | <https://github.com/esposm03/xcursor-rs> |
| `xdg-home` | `1.3.0` | `MIT` | <https://github.com/zeenix/xdg-home> |
| `xkbcommon-dl` | `0.4.2` | `MIT` | <https://github.com/rust-windowing/xkbcommon-dl> |
| `xkeysym` | `0.2.1` | `MIT OR Apache-2.0 OR Zlib` | <https://github.com/notgull/xkeysym> |
| `xml-rs` | `0.8.28` | `MIT` | <https://github.com/kornelski/xml-rs> |
| `yoke` | `0.8.2` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `yoke-derive` | `0.8.2` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `zbus` | `4.4.0` | `MIT` | <https://github.com/dbus2/zbus/> |
| `zbus` | `5.16.0` | `MIT` | <https://github.com/z-galaxy/zbus/> |
| `zbus-lockstep` | `0.4.4` | `MIT` | <https://github.com/luukvanderduim/zbus-lockstep> |
| `zbus-lockstep-macros` | `0.4.4` | `MIT` | <https://github.com/luukvanderduim/zbus-lockstep> |
| `zbus_macros` | `4.4.0` | `MIT` | <https://github.com/dbus2/zbus/> |
| `zbus_macros` | `5.16.0` | `MIT` | <https://github.com/z-galaxy/zbus/> |
| `zbus_names` | `3.0.0` | `MIT` | <https://github.com/dbus2/zbus/> |
| `zbus_names` | `4.3.2` | `MIT` | <https://github.com/z-galaxy/zbus/> |
| `zbus_xml` | `4.0.0` | `MIT` | <https://github.com/dbus2/zbus/> |
| `zerocopy` | `0.8.50` | `BSD-2-Clause OR Apache-2.0 OR MIT` | <https://github.com/google/zerocopy> |
| `zerocopy-derive` | `0.8.50` | `BSD-2-Clause OR Apache-2.0 OR MIT` | <https://github.com/google/zerocopy> |
| `zerofrom` | `0.1.8` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `zerofrom-derive` | `0.1.7` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `zerotrie` | `0.2.4` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `zerovec` | `0.11.6` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `zerovec-derive` | `0.11.3` | `Unicode-3.0` | <https://github.com/unicode-org/icu4x> |
| `zmij` | `1.0.21` | `MIT` | <https://github.com/dtolnay/zmij> |
| `zvariant` | `4.2.0` | `MIT` | <https://github.com/dbus2/zbus/> |
| `zvariant` | `5.12.0` | `MIT` | <https://github.com/z-galaxy/zbus/> |
| `zvariant_derive` | `4.2.0` | `MIT` | <https://github.com/dbus2/zbus/> |
| `zvariant_derive` | `5.12.0` | `MIT` | <https://github.com/z-galaxy/zbus/> |
| `zvariant_utils` | `2.1.0` | `MIT` | <https://github.com/dbus2/zbus/> |
| `zvariant_utils` | `3.4.0` | `MIT` | <https://github.com/z-galaxy/zbus/> |
