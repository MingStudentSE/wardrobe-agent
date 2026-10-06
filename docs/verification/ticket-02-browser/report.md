# Ticket #2 actual browser verification

2026-10-07, Codex in-app browser, Vite loopback dev bridge invoking real wardrobe-cli / SQLite. This is browser GUI evidence, not native Tauri installation evidence.

- Empty/error layout screenshots at 320/375/414/768/1024/1280/1440px. First backend unavailable state rendered honest retry error; corrected technical product copy and narrow direction wording.
- Saved no-image item 验收样例 · 白色棉衬衫, category top, spring/autumn, price 199.90 CNY. GUI displayed success and card values. Full reload retained item.
- Actual keyboard Tab from name reached category SELECT with solid visible outline.
- Persisted-state measurements at all seven widths show document scrollWidth <= innerWidth; main actionable anchors/buttons >=48px high. See persisted-widths.json and screenshots.
- This does not yet prove full native restart, reduced motion, font scaling, long-value handling or three-platform build/install acceptance.

## Actual native macOS run

- Built release Tauri binary and macOS app bundle on local Darwin arm64 with `npm --prefix apps/desktop run tauri build -- --bundles app`; actual app size 11.43 MiB.
- Launched native app via tauri://localhost, created 原生验收 · 深蓝外套 at 299.50 CNY with spring/autumn. Quit and relaunched: item remained. Independent CLI/MCP read identical ID/data.
- Native upgrade exposed early price_minor record rendering NaN; transactional migration fixed original 199.90 CNY without deleting ID/history. Launched rebuilt app and repeated quit/relaunch: both199.90/299.50 displayed correctly. See native-migrated.png.
- This is local macOS ARM execution/build evidence, not three-platform installation/release acceptance.
