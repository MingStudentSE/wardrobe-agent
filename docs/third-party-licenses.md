# 第三方许可清单

本清单由 `scripts/generate-license-inventory.py` 从 lockfile 离线生成,覆盖 Rust workspace、桌面应用(src-tauri,被 workspace exclude、发布桌面包实际携带)与 npm 桌面前端三部分。许可标识取自本机 cargo registry 缓存与 node_modules 元数据,不联网拉取许可全文;缓存缺失的包标记 `unknown` 并在下方单列。

复跑方式:`python3 scripts/generate-license-inventory.py`(输出对相同输入逐字节一致,`--check` 可校验)。

## 输入基线

| 范围 | lockfile | SHA256(前 16 位) | 包数 |
| --- | --- | --- | --- |
| Rust workspace | Cargo.lock | b373abfb6c99deba | 393 |
| 桌面 src-tauri | apps/desktop/src-tauri/Cargo.lock | 2a305b8714703b9b | 643 |
| npm desktop | apps/desktop/package-lock.json | d35227d74a5456b7 | 89 |

> 本清单是元数据盘点,不是法律意见;许可全文以各上游仓库发布文本为准。

## 汇总

| 范围 | 包数 | 许可已知 | unknown | 待审查(非 MIT/Apache/BSD/ISC 家族) |
| --- | --- | --- | --- | --- |
| Rust workspace | 393 | 391 | 2 | 22 |
| 桌面 src-tauri | 643 | 631 | 12 | 37 |
| npm desktop | 89 | 29 | 60 | 60 |
| 合计 | 1125 | 1051 | 74 | 119 |

## Rust workspace依赖许可(393 包)

| 包名 | 版本 | 许可标识 | 来源状态 |
| --- | --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| aead | 0.5.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| aes | 0.8.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ahash | 0.8.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| anymap3 | 1.1.0 | BlueOak-1.0.0 OR MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| argon2 | 0.5.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-io | 2.6.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-process | 2.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-recursion | 1.2.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-task | 4.7.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| base64 | 0.22.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| base64 | 0.23.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| bit-set | 0.11.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| bit-vec | 0.10.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| blake2 | 0.10.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| block-padding | 0.3.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| blocking | 1.7.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| borsh | 1.8.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| borsh-derive | 1.8.1 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| byteorder | 1.5.0 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| bytes | 1.12.1 | MIT | registry 源码缓存 Cargo.toml |
| cbc | 0.1.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cc | 1.6.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cfg-expr | 0.20.10 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cfg_aliases | 0.2.2 | MIT | registry 源码缓存 Cargo.toml |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| chacha20 | 0.9.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| chacha20poly1305 | 0.10.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| chrono | 0.4.45 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| chrono-tz | 0.10.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cipher | 0.4.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cmov | 0.5.4 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| const-oid | 0.9.6 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crunchy | 0.2.4 | MIT | registry 源码缓存 Cargo.toml |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ctutils | 0.4.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| curve25519-dalek | 4.1.3 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| curve25519-dalek-derive | 0.1.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| dbus | 0.9.12 | Apache-2.0/MIT | registry 源码缓存 Cargo.toml |
| dbus-secret-service | 4.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| der | 0.7.10 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| derive-new | 0.7.0 | MIT | registry 源码缓存 Cargo.toml |
| digest | 0.10.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| digest | 0.11.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| downcast-rs | 2.0.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dyn-hash | 1.0.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ed25519 | 2.2.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| ed25519-dalek | 2.2.0 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| either | 1.18.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| endi | 1.1.1 | MIT | registry 源码缓存 Cargo.toml |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| enumn | 0.1.14 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| erased-serde | 0.4.10 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| errno | 0.3.14 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| fiat-crypto | 0.2.9 | MIT OR Apache-2.0 OR BSD-1-Clause | registry 源码缓存 Cargo.toml |
| filetime | 0.2.29 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| foldhash | 0.2.0 | Zlib | registry 源码缓存 Cargo.toml |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| four-cc | 0.4.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| generic-array | 0.14.7 | MIT | registry 源码缓存 Cargo.toml |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| glob | 0.3.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| half | 2.7.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashbrown | 0.14.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashlink | 0.9.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| heck | 0.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hex | 0.4.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hkdf | 0.12.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hkdf | 0.13.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hmac | 0.12.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hmac | 0.13.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| http | 1.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| http-body | 1.1.0 | MIT | registry 源码缓存 Cargo.toml |
| http-body-util | 0.1.5 | MIT | registry 源码缓存 Cargo.toml |
| httparse | 1.10.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hyper | 1.12.0 | MIT | registry 源码缓存 Cargo.toml |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | registry 源码缓存 Cargo.toml |
| hyper-util | 0.1.21 | MIT | registry 源码缓存 Cargo.toml |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| icu_collections | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_locale_core | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_normalizer | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_properties | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_properties_data | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_provider | 2.3.1 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| idna | 1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| image | 0.25.10 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| inout | 0.1.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| inventory | 0.3.24 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| iso_country | 0.1.4 | MIT | registry 源码缓存 Cargo.toml |
| iso_currency | 0.7.1 | MIT | registry 源码缓存 Cargo.toml |
| itertools | 0.14.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| itertools | 0.15.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| itoa | 1.0.18 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| keyring | 3.6.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| libc | 0.2.190 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| libdbus-sys | 0.2.7 | Apache-2.0/MIT | registry 源码缓存 Cargo.toml |
| libheif-rs | 3.0.0 | MIT | registry 源码缓存 Cargo.toml |
| libheif-sys | 5.3.1+1.23.1 | MIT | registry 源码缓存 Cargo.toml |
| libm | 0.2.16 | MIT | registry 源码缓存 Cargo.toml |
| libsqlite3-sys | 0.30.1 | MIT | registry 源码缓存 Cargo.toml |
| libwebp-sys | 0.9.6 | MIT | registry 源码缓存 Cargo.toml |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| litemap | 0.8.3 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| log | 0.4.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib | registry 源码缓存 Cargo.toml |
| maplit | 1.0.2 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| matrixmultiply | 0.3.11 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| memchr | 2.8.3 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| memo-map | 0.3.4 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| memoffset | 0.9.1 | MIT | registry 源码缓存 Cargo.toml |
| minijinja | 2.24.0 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| mio | 1.2.4 | MIT | registry 源码缓存 Cargo.toml |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ndarray | 0.17.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| nix | 0.29.0 | MIT | registry 源码缓存 Cargo.toml |
| nom | 8.0.0 | MIT | registry 源码缓存 Cargo.toml |
| nom-language | 0.1.0 | MIT | registry 源码缓存 Cargo.toml |
| num | 0.4.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-iter | 0.1.46 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-rational | 0.4.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| parking | 2.2.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| password-hash | 0.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| pastey | 0.2.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| phf | 0.12.1 | MIT | registry 源码缓存 Cargo.toml |
| phf_shared | 0.12.1 | MIT | registry 源码缓存 Cargo.toml |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| piper | 0.2.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| pkcs8 | 0.10.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| png | 0.18.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| polling | 3.11.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| poly1305 | 0.8.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| potential_utf | 0.1.6 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| primal-check | 0.3.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| prost | 0.14.4 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| prost-derive | 0.14.4 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| quick-error | 2.0.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| quinn | 0.11.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| quinn-proto | 0.11.19 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| quinn-udp | 0.5.16 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| quote | 1.0.47 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | registry 源码缓存 Cargo.toml |
| rand | 0.10.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand | 0.8.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_distr | 0.6.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rawpointer | 0.2.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| redox_syscall | 0.5.18 | MIT | registry 源码缓存 Cargo.toml |
| reqwest | 0.12.28 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ring | 0.17.14 | Apache-2.0 AND ISC | registry 源码缓存 Cargo.toml |
| rusqlite | 0.32.1 | MIT | registry 源码缓存 Cargo.toml |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rustfft | 6.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| rustls | 0.23.45 | Apache-2.0 OR ISC OR MIT | registry 源码缓存 Cargo.toml |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rustls-webpki | 0.103.15 | ISC | registry 源码缓存 Cargo.toml |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | registry 源码缓存 Cargo.toml |
| same-file | 1.0.6 | Unlicense/MIT | registry 源码缓存 Cargo.toml |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| secret-service | 4.0.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| security-framework | 2.11.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| semver | 1.0.28 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde | 1.0.229 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_spanned | 1.1.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| sha2 | 0.11.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| shlex | 2.0.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| signature | 2.2.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| simd-adler32 | 0.3.10 | MIT | registry 源码缓存 Cargo.toml |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| slab | 0.4.12 | MIT | registry 源码缓存 Cargo.toml |
| smallvec | 1.16.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| spki | 0.7.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| strength_reduce | 0.2.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| string-interner | 0.20.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| subtle | 2.6.1 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| syn | 2.0.119 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| syn | 3.0.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| sync_wrapper | 1.0.2 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| synstructure | 0.14.0 | MIT | registry 源码缓存 Cargo.toml |
| system-deps | 8.0.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tar | 0.4.46 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| target-lexicon | 0.13.5 | Apache-2.0 WITH LLVM-exception | registry 源码缓存 Cargo.toml |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tinystr | 0.8.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tokio | 1.53.2 | MIT | registry 源码缓存 Cargo.toml |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_writer | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tower | 0.5.3 | MIT | registry 源码缓存 Cargo.toml |
| tower-http | 0.6.11 | MIT | registry 源码缓存 Cargo.toml |
| tower-layer | 0.3.3 | MIT | registry 源码缓存 Cargo.toml |
| tower-service | 0.3.3 | MIT | registry 源码缓存 Cargo.toml |
| tracing | 0.1.44 | MIT | registry 源码缓存 Cargo.toml |
| tracing-attributes | 0.1.31 | MIT | registry 源码缓存 Cargo.toml |
| tracing-core | 0.1.36 | MIT | registry 源码缓存 Cargo.toml |
| tract-core | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-data | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-extra | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-hir | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-linalg | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-nnef | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-onnx | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-onnx-opl | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| transpose | 0.2.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| try-lock | 0.2.5 | MIT | registry 源码缓存 Cargo.toml |
| typeid | 1.0.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| typenum | 1.20.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| uds_windows | 1.2.1 | MIT | registry 源码缓存 Cargo.toml |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | registry 源码缓存 Cargo.toml |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| untrusted | 0.9.0 | ISC | registry 源码缓存 Cargo.toml |
| url | 2.5.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| uuid | 1.27.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| vcpkg | 0.2.15 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| version-compare | 0.2.1 | MIT | registry 源码缓存 Cargo.toml |
| version_check | 0.9.5 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| walkdir | 2.5.0 | Unlicense/MIT | registry 源码缓存 Cargo.toml |
| want | 0.3.2 | MIT | registry 源码缓存 Cargo.toml |
| wardrobe-cli | 0.1.0 | unknown | 本项目 crate,Cargo.toml 未声明 license(非第三方) |
| wardrobe-core | 0.1.0 | unknown | 本项目 crate,Cargo.toml 未声明 license(非第三方) |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| web-time | 1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| webp | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | registry 源码缓存 Cargo.toml |
| winapi-util | 0.1.11 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_gnullvm | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_msvc | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnu | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnullvm | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_msvc | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnu | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnullvm | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| winnow | 1.0.4 | MIT | registry 源码缓存 Cargo.toml |
| writeable | 0.6.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| x25519-dalek | 2.0.1 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| xattr | 1.6.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| xdg-home | 1.3.0 | MIT | registry 源码缓存 Cargo.toml |
| yoke | 0.8.3 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| yoke-derive | 0.8.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zbus | 4.4.0 | MIT | registry 源码缓存 Cargo.toml |
| zbus_macros | 4.4.0 | MIT | registry 源码缓存 Cargo.toml |
| zbus_names | 3.0.0 | MIT | registry 源码缓存 Cargo.toml |
| zerocopy | 0.8.60 | BSD-2-Clause OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| zerocopy-derive | 0.8.60 | BSD-2-Clause OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| zerofrom | 0.1.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| zerotrie | 0.2.5 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zerovec | 0.11.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zerovec-derive | 0.11.6 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zlib-rs | 0.6.8 | Zlib | registry 源码缓存 Cargo.toml |
| zmij | 1.0.23 | MIT | registry 源码缓存 Cargo.toml |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | registry 源码缓存 Cargo.toml |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | registry 源码缓存 Cargo.toml |
| zvariant | 4.2.0 | MIT | registry 源码缓存 Cargo.toml |
| zvariant_derive | 4.2.0 | MIT | registry 源码缓存 Cargo.toml |
| zvariant_utils | 2.1.0 | MIT | registry 源码缓存 Cargo.toml |

## 桌面 src-tauri依赖许可(643 包)

| 包名 | 版本 | 许可标识 | 来源状态 |
| --- | --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| aead | 0.5.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| aes | 0.8.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ahash | 0.8.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| aho-corasick | 1.1.5 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| alloc-no-stdlib | 3.0.0 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| alloc-stdlib | 0.3.0 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| anymap3 | 1.1.0 | BlueOak-1.0.0 OR MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| argon2 | 0.5.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-executor | 1.14.0 | unknown | 本地缓存缺失(unknown) |
| async-io | 2.6.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-process | 2.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-recursion | 1.2.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-task | 4.7.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| atk | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| atk-sys | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| base64 | 0.21.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| base64 | 0.22.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| base64 | 0.23.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| bit-set | 0.11.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| bit-vec | 0.10.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| bitflags | 1.3.2 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| blake2 | 0.10.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| block-padding | 0.3.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| block2 | 0.6.2 | MIT | registry 源码缓存 Cargo.toml |
| blocking | 1.7.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| borsh | 1.8.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| borsh-derive | 1.8.1 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| brotli | 9.0.0 | BSD-3-Clause AND MIT | registry 源码缓存 Cargo.toml |
| brotli-decompressor | 6.0.1 | BSD-3-Clause/MIT | registry 源码缓存 Cargo.toml |
| bs58 | 0.5.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| byteorder | 1.5.0 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| bytes | 1.12.1 | MIT | registry 源码缓存 Cargo.toml |
| cairo-rs | 0.18.5 | MIT | registry 源码缓存 Cargo.toml |
| cairo-sys-rs | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| camino | 1.2.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cargo-platform | 0.1.9 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cargo_metadata | 0.19.2 | MIT | registry 源码缓存 Cargo.toml |
| cargo_toml | 1.0.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| cbc | 0.1.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cc | 1.6.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cesu8 | 1.1.0 | Apache-2.0/MIT | registry 源码缓存 Cargo.toml |
| cfb | 0.14.0 | MIT | registry 源码缓存 Cargo.toml |
| cfg-expr | 0.15.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cfg-expr | 0.20.10 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cfg_aliases | 0.2.2 | MIT | registry 源码缓存 Cargo.toml |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| chacha20 | 0.9.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| chacha20poly1305 | 0.10.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| chrono | 0.4.45 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| chrono-tz | 0.10.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cipher | 0.4.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cmov | 0.5.4 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| combine | 4.6.8 | MIT | registry 源码缓存 Cargo.toml |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| const-oid | 0.9.6 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| cookie | 0.18.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| core-graphics | 0.25.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| core-graphics-types | 0.2.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crunchy | 0.2.4 | MIT | registry 源码缓存 Cargo.toml |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| cssparser | 0.37.0 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| cssparser-macros | 0.7.1 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| ctor | 1.0.13 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| ctutils | 0.4.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| curve25519-dalek | 4.1.3 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| curve25519-dalek-derive | 0.1.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| darling | 0.24.1 | MIT | registry 源码缓存 Cargo.toml |
| darling_core | 0.24.1 | MIT | registry 源码缓存 Cargo.toml |
| darling_macro | 0.24.1 | MIT | registry 源码缓存 Cargo.toml |
| dbus | 0.9.12 | Apache-2.0/MIT | registry 源码缓存 Cargo.toml |
| dbus-secret-service | 4.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| defmt | 1.1.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| defmt-macros | 1.1.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| defmt-parser | 1.0.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| der | 0.7.10 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| deranged | 0.5.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| derive-new | 0.7.0 | MIT | registry 源码缓存 Cargo.toml |
| derive_more | 2.1.1 | MIT | registry 源码缓存 Cargo.toml |
| derive_more-impl | 2.1.1 | MIT | registry 源码缓存 Cargo.toml |
| digest | 0.10.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| digest | 0.11.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dirs | 7.0.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dlopen2 | 0.8.2 | MIT | registry 源码缓存 Cargo.toml |
| dlopen2_derive | 0.4.3 | MIT | registry 源码缓存 Cargo.toml |
| dom_query | 0.28.0 | MIT | registry 源码缓存 Cargo.toml |
| downcast-rs | 2.0.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dpi | 0.1.2 | Apache-2.0 AND MIT | registry 源码缓存 Cargo.toml |
| dtoa | 1.0.11 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dtoa-short | 0.3.5 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| dyn-hash | 1.0.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ed25519 | 2.2.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| ed25519-dalek | 2.2.0 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| either | 1.18.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| embed-resource | 3.0.12 | MIT | registry 源码缓存 Cargo.toml |
| embed_plist | 1.2.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| endi | 1.1.1 | MIT | registry 源码缓存 Cargo.toml |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| enumn | 0.1.14 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| erased-serde | 0.4.10 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| errno | 0.3.14 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| fiat-crypto | 0.2.9 | MIT OR Apache-2.0 OR BSD-1-Clause | registry 源码缓存 Cargo.toml |
| field-offset | 0.3.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| filetime | 0.2.29 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| fnv | 1.0.7 | Apache-2.0 / MIT | registry 源码缓存 Cargo.toml |
| foldhash | 0.2.0 | Zlib | registry 源码缓存 Cargo.toml |
| foreign-types | 0.5.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| four-cc | 0.4.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| gdk | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| gdk-pixbuf | 0.18.5 | MIT | registry 源码缓存 Cargo.toml |
| gdk-pixbuf-sys | 0.18.0 | MIT | registry 源码缓存 Cargo.toml |
| gdk-sys | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| gdkwayland-sys | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| gdkx11 | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| gdkx11-sys | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| generic-array | 0.14.7 | MIT | registry 源码缓存 Cargo.toml |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| gio | 0.18.4 | MIT | registry 源码缓存 Cargo.toml |
| gio-sys | 0.18.1 | MIT | registry 源码缓存 Cargo.toml |
| glib | 0.18.5 | MIT | registry 源码缓存 Cargo.toml |
| glib-macros | 0.18.5 | MIT | registry 源码缓存 Cargo.toml |
| glib-sys | 0.18.1 | MIT | registry 源码缓存 Cargo.toml |
| glob | 0.3.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| gobject-sys | 0.18.0 | MIT | registry 源码缓存 Cargo.toml |
| gtk | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| gtk-sys | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| gtk3-macros | 0.18.2 | MIT | registry 源码缓存 Cargo.toml |
| half | 2.7.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashbrown | 0.12.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashbrown | 0.14.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hashlink | 0.9.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| heck | 0.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| heck | 0.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hex | 0.4.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hkdf | 0.12.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hkdf | 0.13.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hmac | 0.12.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hmac | 0.13.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| html5ever | 0.39.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| http | 1.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| http-body | 1.1.0 | MIT | registry 源码缓存 Cargo.toml |
| http-body-util | 0.1.5 | MIT | registry 源码缓存 Cargo.toml |
| httparse | 1.10.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| hyper | 1.12.0 | MIT | registry 源码缓存 Cargo.toml |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | registry 源码缓存 Cargo.toml |
| hyper-util | 0.1.21 | MIT | registry 源码缓存 Cargo.toml |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ico | 0.5.0 | MIT | registry 源码缓存 Cargo.toml |
| icu_collections | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_locale_core | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_normalizer | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_properties | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_properties_data | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| icu_provider | 2.3.1 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| ident_case | 1.0.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| idna | 1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| image | 0.25.10 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| indexmap | 1.9.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| infer | 0.22.0 | MIT | registry 源码缓存 Cargo.toml |
| inout | 0.1.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| inventory | 0.3.24 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| is-docker | 0.2.0 | unknown | 本地缓存缺失(unknown) |
| is-wsl | 0.4.0 | unknown | 本地缓存缺失(unknown) |
| iso_country | 0.1.4 | MIT | registry 源码缓存 Cargo.toml |
| iso_currency | 0.7.1 | MIT | registry 源码缓存 Cargo.toml |
| itertools | 0.14.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| itertools | 0.15.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| itoa | 1.0.18 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| javascriptcore-rs | 1.1.2 | MIT | registry 源码缓存 Cargo.toml |
| javascriptcore-rs-sys | 1.1.1 | MIT | registry 源码缓存 Cargo.toml |
| jiff | 0.2.38 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| jiff-core | 0.1.1 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| jiff-static | 0.2.38 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| jiff-tzdb | 0.1.9 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| jiff-tzdb-platform | 0.1.3 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| jni | 0.21.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| json-patch | 4.2.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| jsonptr | 0.7.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| keyboard-types | 0.8.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| keyring | 3.6.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| libappindicator | 0.9.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| libappindicator-sys | 0.9.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| libc | 0.2.190 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| libdbus-sys | 0.2.7 | Apache-2.0/MIT | registry 源码缓存 Cargo.toml |
| libheif-rs | 3.0.0 | MIT | registry 源码缓存 Cargo.toml |
| libheif-sys | 5.3.1+1.23.1 | MIT | registry 源码缓存 Cargo.toml |
| libloading | 0.7.4 | ISC | registry 源码缓存 Cargo.toml |
| libm | 0.2.16 | MIT | registry 源码缓存 Cargo.toml |
| libredox | 0.1.25 | MIT | registry 源码缓存 Cargo.toml |
| libsqlite3-sys | 0.30.1 | MIT | registry 源码缓存 Cargo.toml |
| libwebp-sys | 0.9.6 | MIT | registry 源码缓存 Cargo.toml |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| litemap | 0.8.3 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| log | 0.4.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib | registry 源码缓存 Cargo.toml |
| maplit | 1.0.2 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| markup5ever | 0.39.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| matrixmultiply | 0.3.11 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| memchr | 2.8.3 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| memo-map | 0.3.4 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| memoffset | 0.9.1 | MIT | registry 源码缓存 Cargo.toml |
| mime | 0.3.17 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| minijinja | 2.24.0 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| mio | 1.2.4 | MIT | registry 源码缓存 Cargo.toml |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| muda | 0.20.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| ndarray | 0.17.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ndk | 0.9.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| new_debug_unreachable | 1.0.6 | MIT | registry 源码缓存 Cargo.toml |
| nix | 0.29.0 | MIT | registry 源码缓存 Cargo.toml |
| nom | 8.0.0 | MIT | registry 源码缓存 Cargo.toml |
| nom-language | 0.1.0 | MIT | registry 源码缓存 Cargo.toml |
| num | 0.4.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-iter | 0.1.46 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-rational | 0.4.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| objc2 | 0.6.5 | MIT | registry 源码缓存 Cargo.toml |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-cloud-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-core-data | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-core-image | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-core-location | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-core-text | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-encode | 4.1.0 | MIT | registry 源码缓存 Cargo.toml |
| objc2-exception-helper | 0.1.1 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-foundation | 0.3.2 | MIT | registry 源码缓存 Cargo.toml |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-ui-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-user-notifications | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| objc2-web-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| open | 5.4.4 | MIT | registry 源码缓存 Cargo.toml |
| option-ext | 0.2.0 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| pango | 0.18.3 | MIT | registry 源码缓存 Cargo.toml |
| pango-sys | 0.18.0 | MIT | registry 源码缓存 Cargo.toml |
| parking | 2.2.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| password-hash | 0.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| pastey | 0.2.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| phf | 0.12.1 | MIT | registry 源码缓存 Cargo.toml |
| phf | 0.13.1 | MIT | registry 源码缓存 Cargo.toml |
| phf_codegen | 0.13.1 | MIT | registry 源码缓存 Cargo.toml |
| phf_generator | 0.13.1 | MIT | registry 源码缓存 Cargo.toml |
| phf_macros | 0.13.1 | MIT | registry 源码缓存 Cargo.toml |
| phf_shared | 0.12.1 | MIT | registry 源码缓存 Cargo.toml |
| phf_shared | 0.13.1 | MIT | registry 源码缓存 Cargo.toml |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| piper | 0.2.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| pkcs8 | 0.10.2 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| plist | 1.10.1 | MIT | registry 源码缓存 Cargo.toml |
| png | 0.17.16 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| png | 0.18.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| polling | 3.11.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| poly1305 | 0.8.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| potential_utf | 0.1.6 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| precomputed-hash | 0.1.1 | MIT | registry 源码缓存 Cargo.toml |
| primal-check | 0.3.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| proc-macro-crate | 1.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| proc-macro-crate | 2.0.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| proc-macro-error | 1.0.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| proc-macro-error-attr | 1.0.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| prost | 0.14.4 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| prost-derive | 0.14.4 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| quick-error | 2.0.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| quick-xml | 0.42.0 | MIT | registry 源码缓存 Cargo.toml |
| quinn | 0.11.12 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| quinn-proto | 0.11.19 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| quinn-udp | 0.5.16 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| quote | 1.0.47 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | registry 源码缓存 Cargo.toml |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | registry 源码缓存 Cargo.toml |
| rand | 0.10.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand | 0.8.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_distr | 0.6.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | registry 源码缓存 Cargo.toml |
| rawpointer | 0.2.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| redox_syscall | 0.5.18 | MIT | registry 源码缓存 Cargo.toml |
| redox_users | 0.5.3 | MIT | registry 源码缓存 Cargo.toml |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| regex | 1.13.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| reqwest | 0.12.28 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| reqwest | 0.13.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rfd | 0.16.0 | MIT | registry 源码缓存 Cargo.toml |
| ring | 0.17.14 | Apache-2.0 AND ISC | registry 源码缓存 Cargo.toml |
| rusqlite | 0.32.1 | MIT | registry 源码缓存 Cargo.toml |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rustfft | 6.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| rustls | 0.23.45 | Apache-2.0 OR ISC OR MIT | registry 源码缓存 Cargo.toml |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| rustls-webpki | 0.103.15 | ISC | registry 源码缓存 Cargo.toml |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | registry 源码缓存 Cargo.toml |
| same-file | 1.0.6 | Unlicense/MIT | registry 源码缓存 Cargo.toml |
| schemars | 0.8.22 | MIT | registry 源码缓存 Cargo.toml |
| schemars | 0.9.0 | MIT | registry 源码缓存 Cargo.toml |
| schemars | 1.2.2 | MIT | registry 源码缓存 Cargo.toml |
| schemars_derive | 0.8.22 | MIT | registry 源码缓存 Cargo.toml |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| secret-service | 4.0.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| security-framework | 2.11.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| selectors | 0.38.0 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| semver | 1.0.28 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde | 1.0.229 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde-untagged | 0.1.9 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_derive_internals | 0.29.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_spanned | 0.6.9 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_spanned | 1.1.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_with | 3.24.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serde_with_macros | 3.24.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serialize-to-javascript | 0.1.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| serialize-to-javascript-impl | 0.1.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| servo_arc | 0.4.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| sha2 | 0.11.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| shlex | 2.0.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| signature | 2.2.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| simd-adler32 | 0.3.10 | MIT | registry 源码缓存 Cargo.toml |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| slab | 0.4.12 | MIT | registry 源码缓存 Cargo.toml |
| smallvec | 1.16.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| softbuffer | 0.4.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| soup3 | 0.5.0 | MIT | registry 源码缓存 Cargo.toml |
| soup3-sys | 0.5.0 | MIT | registry 源码缓存 Cargo.toml |
| spki | 0.7.3 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| strength_reduce | 0.2.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| string-interner | 0.20.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| string_cache | 0.9.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| string_cache_codegen | 0.6.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| strsim | 0.11.1 | MIT | registry 源码缓存 Cargo.toml |
| subtle | 2.6.1 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| swift-rs | 1.0.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| syn | 1.0.109 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| syn | 2.0.119 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| syn | 3.0.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| sync_wrapper | 1.0.2 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| synstructure | 0.14.0 | MIT | registry 源码缓存 Cargo.toml |
| system-deps | 6.2.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| system-deps | 8.0.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tao | 0.37.1 | Apache-2.0 | registry 源码缓存 Cargo.toml |
| tao-macros | 0.1.4 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tar | 0.4.46 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| target-lexicon | 0.12.16 | Apache-2.0 WITH LLVM-exception | registry 源码缓存 Cargo.toml |
| target-lexicon | 0.13.5 | Apache-2.0 WITH LLVM-exception | registry 源码缓存 Cargo.toml |
| tauri | 2.12.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-build | 2.7.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-codegen | 2.7.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-macros | 2.7.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-plugin | 2.7.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-plugin-dialog | 2.8.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-plugin-fs | 2.6.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-plugin-opener | 2.7.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-runtime | 2.12.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-runtime-wry | 2.12.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-utils | 2.10.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tauri-winres | 0.3.6 | MIT | registry 源码缓存 Cargo.toml |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tendril | 0.5.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| time | 0.3.55 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| time-core | 0.1.9 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tinystr | 0.8.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| tokio | 1.53.2 | MIT | registry 源码缓存 Cargo.toml |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tokio-util | 0.7.19 | MIT | registry 源码缓存 Cargo.toml |
| toml | 0.8.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_datetime | 0.6.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_edit | 0.19.15 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_edit | 0.20.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| toml_writer | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tower | 0.5.3 | MIT | registry 源码缓存 Cargo.toml |
| tower-http | 0.6.11 | MIT | registry 源码缓存 Cargo.toml |
| tower-layer | 0.3.3 | MIT | registry 源码缓存 Cargo.toml |
| tower-service | 0.3.3 | MIT | registry 源码缓存 Cargo.toml |
| tracing | 0.1.44 | MIT | registry 源码缓存 Cargo.toml |
| tracing-attributes | 0.1.31 | MIT | registry 源码缓存 Cargo.toml |
| tracing-core | 0.1.36 | MIT | registry 源码缓存 Cargo.toml |
| tract-core | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-data | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-extra | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-hir | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-linalg | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-nnef | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-onnx | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tract-onnx-opl | 0.23.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| transpose | 0.2.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| tray-icon | 0.25.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| try-lock | 0.2.5 | MIT | registry 源码缓存 Cargo.toml |
| typeid | 1.0.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| typenum | 1.20.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| uds_windows | 1.2.1 | MIT | registry 源码缓存 Cargo.toml |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | registry 源码缓存 Cargo.toml |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| untrusted | 0.9.0 | ISC | registry 源码缓存 Cargo.toml |
| url | 2.5.8 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| urlpattern | 0.6.0 | MIT | registry 源码缓存 Cargo.toml |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| uuid | 1.27.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| vcpkg | 0.2.15 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| version-compare | 0.2.1 | MIT | registry 源码缓存 Cargo.toml |
| version_check | 0.9.5 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| vswhom | 0.1.0 | MIT | registry 源码缓存 Cargo.toml |
| vswhom-sys | 0.1.3 | MIT | registry 源码缓存 Cargo.toml |
| walkdir | 2.5.0 | Unlicense/MIT | registry 源码缓存 Cargo.toml |
| want | 0.3.2 | MIT | registry 源码缓存 Cargo.toml |
| wardrobe-core | 0.1.0 | unknown | 本项目 crate,Cargo.toml 未声明 license(非第三方) |
| wardrobe-desktop | 0.1.0 | unknown | 本项目 crate,Cargo.toml 未声明 license(非第三方) |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| wasm-streams | 0.5.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| web-time | 1.1.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| web_atoms | 0.2.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| webkit2gtk | 2.0.2 | MIT | registry 源码缓存 Cargo.toml |
| webkit2gtk-sys | 2.0.2 | MIT | registry 源码缓存 Cargo.toml |
| webp | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | registry 源码缓存 Cargo.toml |
| webview2-com | 0.39.1 | MIT | registry 源码缓存 Cargo.toml |
| webview2-com-macros | 0.8.1 | MIT | registry 源码缓存 Cargo.toml |
| webview2-com-sys | 0.39.1 | MIT | registry 源码缓存 Cargo.toml |
| winapi | 0.3.9 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| winapi-i686-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| winapi-util | 0.1.11 | Unlicense OR MIT | registry 源码缓存 Cargo.toml |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 | registry 源码缓存 Cargo.toml |
| window-vibrancy | 0.8.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| windows | 0.62.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.45.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-targets | 0.42.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows-version | 0.1.7 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_gnullvm | 0.42.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_gnullvm | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_msvc | 0.42.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_aarch64_msvc | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnu | 0.42.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnu | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_gnullvm | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_msvc | 0.42.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_i686_msvc | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnu | 0.42.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnu | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnullvm | 0.42.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_gnullvm | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_msvc | 0.42.2 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| winnow | 0.5.40 | MIT | registry 源码缓存 Cargo.toml |
| winnow | 1.0.4 | MIT | registry 源码缓存 Cargo.toml |
| winreg | 0.56.0 | MIT | registry 源码缓存 Cargo.toml |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| writeable | 0.6.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| wry | 0.57.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| x11 | 2.21.0 | MIT | registry 源码缓存 Cargo.toml |
| x11-dl | 2.21.0 | MIT | registry 源码缓存 Cargo.toml |
| x25519-dalek | 2.0.1 | BSD-3-Clause | registry 源码缓存 Cargo.toml |
| xattr | 1.6.1 | MIT OR Apache-2.0 | registry 源码缓存 Cargo.toml |
| xdg-home | 1.3.0 | MIT | registry 源码缓存 Cargo.toml |
| yoke | 0.8.3 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| yoke-derive | 0.8.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zbus | 4.4.0 | MIT | registry 源码缓存 Cargo.toml |
| zbus | 5.19.0 | unknown | 本地缓存缺失(unknown) |
| zbus_macros | 4.4.0 | MIT | registry 源码缓存 Cargo.toml |
| zbus_macros | 5.19.0 | unknown | 本地缓存缺失(unknown) |
| zbus_names | 3.0.0 | MIT | registry 源码缓存 Cargo.toml |
| zbus_names | 4.3.4 | unknown | 本地缓存缺失(unknown) |
| zcheapstr | 1.1.0 | unknown | 本地缓存缺失(unknown) |
| zerocopy | 0.8.60 | BSD-2-Clause OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| zerocopy-derive | 0.8.60 | BSD-2-Clause OR Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| zerofrom | 0.1.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | registry 源码缓存 Cargo.toml |
| zerotrie | 0.2.5 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zerovec | 0.11.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zerovec-derive | 0.11.6 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| zlib-rs | 0.6.8 | Zlib | registry 源码缓存 Cargo.toml |
| zmij | 1.0.23 | MIT | registry 源码缓存 Cargo.toml |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | registry 源码缓存 Cargo.toml |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | registry 源码缓存 Cargo.toml |
| zvariant | 4.2.0 | MIT | registry 源码缓存 Cargo.toml |
| zvariant | 5.15.0 | unknown | 本地缓存缺失(unknown) |
| zvariant_derive | 4.2.0 | MIT | registry 源码缓存 Cargo.toml |
| zvariant_derive | 5.15.0 | unknown | 本地缓存缺失(unknown) |
| zvariant_utils | 2.1.0 | MIT | registry 源码缓存 Cargo.toml |
| zvariant_utils | 4.2.0 | unknown | 本地缓存缺失(unknown) |

## npm desktop依赖许可(89 包)

| 包名 | 版本 | 许可标识 | 来源状态 |
| --- | --- | --- | --- |
| @esbuild/aix-ppc64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/android-arm | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/android-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/android-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/darwin-arm64 | 0.25.12 | MIT | node_modules package.json |
| @esbuild/darwin-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/freebsd-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/freebsd-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-arm | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-ia32 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-loong64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-mips64el | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-ppc64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-riscv64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-s390x | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/linux-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/netbsd-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/netbsd-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/openbsd-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/openbsd-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/openharmony-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/sunos-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/win32-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/win32-ia32 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @esbuild/win32-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| @napi-rs/lzma-linux-x64-gnu | 1.5.1 | unknown | node_modules 目录缺失(unknown) |
| @playwright/test | 1.63.0 | Apache-2.0 | node_modules package.json |
| @rollup/rollup-android-arm-eabi | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-android-arm64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-darwin-arm64 | 4.64.0 | MIT | node_modules package.json |
| @rollup/rollup-darwin-x64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-freebsd-arm64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-freebsd-x64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-arm-gnueabihf | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-arm-musleabihf | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-arm64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-arm64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-loong64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-loong64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-ppc64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-ppc64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-riscv64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-riscv64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-s390x-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-x64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-linux-x64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-openbsd-x64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-openharmony-arm64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-win32-arm64-msvc | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-win32-ia32-msvc | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-win32-x64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @rollup/rollup-win32-x64-msvc | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/api | 2.12.1 | Apache-2.0 OR MIT | node_modules package.json |
| @tauri-apps/cli | 2.12.1 | Apache-2.0 OR MIT | node_modules package.json |
| @tauri-apps/cli-darwin-arm64 | 2.12.1 | Apache-2.0 OR MIT | node_modules package.json |
| @tauri-apps/cli-darwin-x64 | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-linux-arm-gnueabihf | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-linux-arm64-gnu | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-linux-arm64-musl | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-linux-riscv64-gnu | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-linux-x64-gnu | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-linux-x64-musl | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-win32-arm64-msvc | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-win32-ia32-msvc | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/cli-win32-x64-msvc | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| @tauri-apps/plugin-dialog | 2.8.1 | MIT OR Apache-2.0 | node_modules package.json |
| @tauri-apps/plugin-opener | 2.7.0 | MIT OR Apache-2.0 | node_modules package.json |
| @types/estree | 1.0.9 | MIT | node_modules package.json |
| @types/react | 19.3.0 | MIT | node_modules package.json |
| @types/react-dom | 19.3.0 | MIT | node_modules package.json |
| csstype | 3.2.3 | MIT | node_modules package.json |
| esbuild | 0.25.12 | MIT | node_modules package.json |
| fdir | 6.5.0 | MIT | node_modules package.json |
| fsevents | 2.3.3 | MIT | node_modules package.json |
| nanoid | 3.3.20 | MIT | node_modules package.json |
| picocolors | 1.1.1 | ISC | node_modules package.json |
| picomatch | 4.0.7 | MIT | node_modules package.json |
| playwright | 1.63.0 | Apache-2.0 | node_modules package.json |
| playwright-core | 1.63.0 | Apache-2.0 | node_modules package.json |
| postcss | 8.5.29 | MIT | node_modules package.json |
| react | 19.3.0 | MIT | node_modules package.json |
| react-dom | 19.3.0 | MIT | node_modules package.json |
| rollup | 4.64.0 | MIT | node_modules package.json |
| scheduler | 0.28.0 | MIT | node_modules package.json |
| source-map-js | 1.2.2 | BSD-3-Clause | node_modules package.json |
| tinyglobby | 0.2.17 | MIT | node_modules package.json |
| typescript | 5.9.3 | Apache-2.0 | node_modules package.json |
| vite | 6.4.4 | MIT | node_modules package.json |

## 非宽松许可依赖(许可证兼容审查名单)

以下依赖的许可标识不在 MIT / Apache-2.0 / BSD 家族 / ISC / MIT 变体白名单内(含 unknown),按 issue #56 要求单独列出供许可证选择与发行审查。注意:此分类刻意保守——`Unicode-3.0`、`CC0-1.0`、`MPL-2.0`、`zlib` 等事实宽松或弱 copyleft 许可也会列入,列入不代表必然不兼容,发行前需逐项确认。本仓库自有 crate 不属于第三方,不列入本节(见 unknown 待补全)。

| 范围 | 包名 | 版本 | 许可标识 | 来源状态 |
| --- | --- | --- | --- | --- |
| Rust workspace | foldhash | 0.2.0 | Zlib | registry 源码缓存 Cargo.toml |
| Rust workspace | icu_collections | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | icu_locale_core | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | icu_normalizer | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | icu_normalizer_data | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | icu_properties | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | icu_properties_data | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | icu_provider | 2.3.1 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | litemap | 0.8.3 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | potential_utf | 0.1.6 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | tinystr | 0.8.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | writeable | 0.6.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | yoke | 0.8.3 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | yoke-derive | 0.8.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | zerofrom | 0.1.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | zerofrom-derive | 0.1.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | zerotrie | 0.2.5 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | zerovec | 0.11.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | zerovec-derive | 0.11.6 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| Rust workspace | zlib-rs | 0.6.8 | Zlib | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | async-executor | 1.14.0 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | cssparser | 0.37.0 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | cssparser-macros | 0.7.1 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | dtoa-short | 0.3.5 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | foldhash | 0.2.0 | Zlib | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | icu_collections | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | icu_locale_core | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | icu_normalizer | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | icu_normalizer_data | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | icu_properties | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | icu_properties_data | 2.3.0 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | icu_provider | 2.3.1 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | is-docker | 0.2.0 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | is-wsl | 0.4.0 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | litemap | 0.8.3 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | option-ext | 0.2.0 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | potential_utf | 0.1.6 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | selectors | 0.38.0 | MPL-2.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | tinystr | 0.8.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | writeable | 0.6.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | yoke | 0.8.3 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | yoke-derive | 0.8.4 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | zbus | 5.19.0 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zbus_macros | 5.19.0 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zbus_names | 4.3.4 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zcheapstr | 1.1.0 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zerofrom | 0.1.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | zerofrom-derive | 0.1.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | zerotrie | 0.2.5 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | zerovec | 0.11.8 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | zerovec-derive | 0.11.6 | Unicode-3.0 | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | zlib-rs | 0.6.8 | Zlib | registry 源码缓存 Cargo.toml |
| 桌面 src-tauri | zvariant | 5.15.0 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zvariant_derive | 5.15.0 | unknown | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zvariant_utils | 4.2.0 | unknown | 本地缓存缺失(unknown) |
| npm desktop | @esbuild/aix-ppc64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/android-arm | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/android-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/android-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/darwin-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/freebsd-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/freebsd-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-arm | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-ia32 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-loong64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-mips64el | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-ppc64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-riscv64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-s390x | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/netbsd-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/netbsd-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/openbsd-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/openbsd-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/openharmony-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/sunos-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/win32-arm64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/win32-ia32 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/win32-x64 | 0.25.12 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @napi-rs/lzma-linux-x64-gnu | 1.5.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-android-arm-eabi | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-android-arm64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-darwin-x64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-freebsd-arm64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-freebsd-x64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-arm-gnueabihf | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-arm-musleabihf | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-arm64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-arm64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-loong64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-loong64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-ppc64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-ppc64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-riscv64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-riscv64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-s390x-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-x64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-x64-musl | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-openbsd-x64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-openharmony-arm64 | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-win32-arm64-msvc | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-win32-ia32-msvc | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-win32-x64-gnu | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-win32-x64-msvc | 4.64.0 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-darwin-x64 | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-arm-gnueabihf | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-arm64-gnu | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-arm64-musl | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-riscv64-gnu | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-x64-gnu | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-x64-musl | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-win32-arm64-msvc | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-win32-ia32-msvc | 2.12.1 | unknown | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-win32-x64-msvc | 2.12.1 | unknown | node_modules 目录缺失(unknown) |

## unknown 待补全

| 范围 | 包名 | 版本 | 原因 |
| --- | --- | --- | --- |
| Rust workspace | wardrobe-cli | 0.1.0 | 本项目 crate,Cargo.toml 未声明 license(非第三方) |
| Rust workspace | wardrobe-core | 0.1.0 | 本项目 crate,Cargo.toml 未声明 license(非第三方) |
| 桌面 src-tauri | async-executor | 1.14.0 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | is-docker | 0.2.0 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | is-wsl | 0.4.0 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | wardrobe-core | 0.1.0 | 本项目 crate,Cargo.toml 未声明 license(非第三方) |
| 桌面 src-tauri | wardrobe-desktop | 0.1.0 | 本项目 crate,Cargo.toml 未声明 license(非第三方) |
| 桌面 src-tauri | zbus | 5.19.0 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zbus_macros | 5.19.0 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zbus_names | 4.3.4 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zcheapstr | 1.1.0 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zvariant | 5.15.0 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zvariant_derive | 5.15.0 | 本地缓存缺失(unknown) |
| 桌面 src-tauri | zvariant_utils | 4.2.0 | 本地缓存缺失(unknown) |
| npm desktop | @esbuild/aix-ppc64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/android-arm | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/android-arm64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/android-x64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/darwin-x64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/freebsd-arm64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/freebsd-x64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-arm | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-arm64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-ia32 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-loong64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-mips64el | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-ppc64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-riscv64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-s390x | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/linux-x64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/netbsd-arm64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/netbsd-x64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/openbsd-arm64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/openbsd-x64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/openharmony-arm64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/sunos-x64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/win32-arm64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/win32-ia32 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @esbuild/win32-x64 | 0.25.12 | node_modules 目录缺失(unknown) |
| npm desktop | @napi-rs/lzma-linux-x64-gnu | 1.5.1 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-android-arm-eabi | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-android-arm64 | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-darwin-x64 | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-freebsd-arm64 | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-freebsd-x64 | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-arm-gnueabihf | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-arm-musleabihf | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-arm64-gnu | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-arm64-musl | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-loong64-gnu | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-loong64-musl | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-ppc64-gnu | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-ppc64-musl | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-riscv64-gnu | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-riscv64-musl | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-s390x-gnu | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-x64-gnu | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-linux-x64-musl | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-openbsd-x64 | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-openharmony-arm64 | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-win32-arm64-msvc | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-win32-ia32-msvc | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-win32-x64-gnu | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @rollup/rollup-win32-x64-msvc | 4.64.0 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-darwin-x64 | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-arm-gnueabihf | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-arm64-gnu | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-arm64-musl | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-riscv64-gnu | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-x64-gnu | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-linux-x64-musl | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-win32-arm64-msvc | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-win32-ia32-msvc | 2.12.1 | node_modules 目录缺失(unknown) |
| npm desktop | @tauri-apps/cli-win32-x64-msvc | 2.12.1 | node_modules 目录缺失(unknown) |

补全方法:在联网环境执行 `cargo fetch`(Rust)与 `npm ci`(npm)后复跑本脚本;或运行 `cargo about generate` / `license-checker` 等工具交叉核对。补全前不得视为完整发行清单。

注:标注「本项目 crate」的条目是本仓库自有代码而非第三方依赖,其 license 字段未声明属于产品许可证选择事项(见 2026-10-07 MVP 发布门槛记录 `wardrobe-execution/results/continue-20261007/mvp-release-20261007.md`,外盘只读产物,不在仓库版本管理内),不随第三方补全解决。

## Windows MSVC CRT 再分发条款

Windows 构建使用 `x86_64-pc-windows-msvc` 目标(见 2026-10-07 MVP 发布门槛记录,`wardrobe-execution/results/continue-20261007/mvp-release-20261007.md`,外盘只读产物,不在仓库版本管理内),产物依赖 Microsoft Visual C++ 运行库(MSVC CRT:`vcruntime140.dll`、`vcruntime140_1.dll`、`msvcp140.dll` 等)与通用 C 运行库(UCRT:`ucrtbase.dll` 及 `api-ms-win-*` 转发 DLL)。

- MSVC CRT 是**微软许可软件,不是开源软件**,不在上方许可清单内;随安装包再分发必须遵循微软的软件许可条款。
- 微软的 Visual Studio / Visual C++ Redistributable 许可条款允许在遵守条款的前提下随应用再分发 CRT(例如捆绑 `vc_redist.x64.exe`,或按条款直接分发所需 DLL):不得修改、不得绕过许可与激活机制、须随包附带相应许可与声明信息,并仅以运行应用所需为限。
- 若 Tauri/NSIS 打包改为静态链接 CRT(`/MT`)或要求用户自行安装 VC++ 可再发行组件包,再分发义务随之变化,打包方式变更时必须复核本节。
- 官方文档(细节以微软现行条款为准):
  - 再分发 Visual C++ 文件:<https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files>
  - 确定可再分发组件:<https://learn.microsoft.com/cpp/windows/determining-which-dlls-to-redistribute>
  - 受支持 VC++ 可再发行组件包下载:<https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist>

