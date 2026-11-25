# Handoff log — 舊版 STATE 依時間附加於此，只供 grep，resume 不整份讀

<!-- superseded 2026-11-18 20:28 sha 0650fdb6cdeac84d -->
# Peekabrowser macOS — iPad UI parity (branch ui-ipad-parity)

更新：2026-11-18

## 現在在做什麼
- Branch `ui-ipad-parity` (HEAD 0ee0364, not pushed, no PR yet) ports iPad design to the Tauri macOS app:
  - Tide tokens in `src/theme.css`
  - 76px rail plus a top "chrome" NSPanel (`chrome.html`): address bar, nav buttons, tab row
  - per-destination tabs (20/dest + reopen stack)
  - iPad-style settings: text_from_other_apps last_used/fixed/choose, auto_submit
  - opt-in three-finger tap lookup: `hotkeys/three_finger.rs` (MultitouchSupport dlopen) + `selection.rs` (AX tiers + WebKit text markers)
  - animated highlight `panel/lookup_highlight.rs` + `highlight.html`
- Fable review findings were all fixed; ACL manifest splits capabilities (`page-*` gets 4 commands only).
- Signed release build is installed in /Applications (user confirmed: three-finger works in Safari).

## 卡住的點
- (none)

## 下一步
1. Improve the three-finger highlight animation: the cursor-ring "dot" fallback is unintuitive and sometimes hard to see; make the box more visible too. Files: `src/highlight.css`, `panel/lookup_highlight.rs`. Fable may be consulted for design.
2. Bug: ⌘W closing a tab makes the top chrome bar (address + buttons) disappear. Look at `panel::sync_chrome` / `lifecycle::close_page` and the chrome show/hide paths.
3. Rebuild + reinstall (recipe in project memory `release-build-strip-quirk`), the user retests, then open the PR.

## 未結案事項
- [O1] Highlight animation redesign (dot fallback unclear, box not prominent enough)
- [O2] ⌘W makes top chrome bar disappear
- [O3] Manual checks still pending: three-finger on selected text / Chrome; 注音 Enter in address bar; ⌘T/⌘W/⌘⇧T inside pages; pin + picker + S/M/L widths
- [O4] Open PR to fireman333/peekabrowser after user testing (not pushed yet)
- [O5] iOS session (Resume spec 效能優化) queued: add macOS download link (releases/latest) to iPad product page

## 本次結案
- (first STATE)

## 指標
- Design brief: iOS-Peekabrowser/research/19-ui-overhaul-spec.md
- Review: scratchpad fable-review.md (findings all fixed in 3bc11b8..8ee0cf6)
- Log file of installed app: ~/Library/Logs/Peekabrowser/peekabrowser.log (one line per lookup with tier diagnostics)
- Local release build needs CARGO_PROFILE_RELEASE_STRIP=false (project memory release-build-strip-quirk); old 2.0.6 app backup in release/backup/
- Debug: `PB_DEBUG_SHOW=1 npx tauri dev` auto-shows the panel; live AX test: `PEEKA_AT=x,y cargo test --lib live_capture -- --ignored --nocapture`
- ~/.claude edits this session: only project memory release-build-strip-quirk.md + MEMORY.md line

<!-- superseded 2026-11-18 20:31 sha 96894a48109573e1 -->
# Peekabrowser macOS — v2.1.1 released (iPad UI parity shipped)

更新：2026-11-18

## 現在在做什麼
- Nothing in progress. PR #9 (ui-ipad-parity) merged into main; v2.1.0 and v2.1.1 published on GitHub (notarized DMGs, arm64 + x64). /Applications has notarized 2.1.1.
- Shipped this session: 70px rail; bottom-right resize grip (fades in near corner, top-left fixed; S/M/L presets kept); ⌘W chrome blink fix (CHROME_HOLD); "sent" chip beside pointer on auto-send (three-finger + ⌘C+C; no overlay when picker opens; text-box highlight removed); iOS prompts + 7 profession templates (picker: hold / right-click / → ; settings editors; prompts.rs); last tab closable (empty.html empty state, chrome hidden); picker inner outline removed; destinations "重新排序"→"編輯"; app icon = iOS AppIcon.icon (ictool macOS rendition, 824/1024 grid); settings switch off-track color; CI runner macos-14→macos-15.

## 卡住的點
- (none)

## 下一步
1. Remaining manual checks (O3): three-finger on selected text / Chrome; 注音 Enter in address bar; pin + picker + S/M/L.
2. Check in-app updater actually upgrades v2.0.x → v2.1.1 (not tested).
3. iOS session: add macOS download link to iPad product page (O5).

## 未結案事項
- [O3] Manual checks still pending: three-finger on selected text / Chrome; 注音 Enter in address bar; pin + picker + S/M/L widths
- [O5] iOS session (Resume spec 效能優化) queued: add macOS download link (releases/latest) to iPad product page
- [O6] In-app updater path v2.0.x → v2.1.x never tested end to end

## 本次結案
- [O1] Highlight animation redesign — replaced by picker-styled "sent" chip on direct send; box/dot removed (user approved)
- [O2] ⌘W makes top chrome bar disappear — fixed with CHROME_HOLD during close + visibility re-check; user confirmed OK
- [O4] Open PR — PR #9 opened, CI green on macos-15, merged; released as v2.1.0/v2.1.1

## 指標
- Release recipe: `CARGO_PROFILE_RELEASE_STRIP=false SQLITE3_LIB_DIR=$(xcrun --show-sdk-path)/usr/lib ./scripts/notarize-release.sh <ver>` → `gh release create v<ver> release/v<ver>/* --notes-file docs/releases/v<ver>.md` (notes bilingual; bump package.json, tauri.conf.json, Cargo.toml)
- Local install: ditto bundle/macos/Peekabrowser.app to /Applications (project memory release-build-strip-quirk)
- Repo has no auto-merge enabled; CI publishes nothing unless workflow_dispatch with release_tag
- User data lives only in ~/Library/Application Support/com.peekabrowser.app (not bundled; verified)
- iOS prompt model reference: iOS-Peekabrowser App/Sources/Model/QueryLens.swift; macOS port src-tauri/src/prompts.rs

<!-- superseded 2026-11-19 14:39 sha f70e3c08fbf155c5 -->
# Peekabrowser macOS — v2.1.1 released (iPad UI parity shipped)

更新：2026-11-18

## 現在在做什麼
- Nothing in progress. PR #9 (ui-ipad-parity) merged into main; v2.1.0 and v2.1.1 published on GitHub (notarized DMGs, arm64 + x64). /Applications has notarized 2.1.1.
- Shipped this session: 70px rail; bottom-right resize grip (fades in near corner, top-left fixed; S/M/L presets kept); ⌘W chrome blink fix (CHROME_HOLD); "sent" chip beside pointer on auto-send (three-finger + ⌘C+C; no overlay when picker opens; text-box highlight removed); iOS prompts + 7 profession templates (picker: hold / right-click / → ; settings editors; prompts.rs); last tab closable (empty.html empty state, chrome hidden); picker inner outline removed; destinations "重新排序"→"編輯"; app icon = iOS AppIcon.icon (ictool macOS rendition, 824/1024 grid); settings switch off-track color; CI runner macos-14→macos-15.

## 卡住的點
- (none)

## 下一步
1. Remaining manual checks (O3): three-finger on selected text / Chrome; 注音 Enter in address bar; pin + picker + S/M/L.
2. Check in-app updater actually upgrades v2.0.x → v2.1.1 (not tested).
3. iOS session: add macOS download link to iPad product page (O5) — owned by that session; nothing to do here.

## 未結案事項
- [O3] Manual checks still pending: three-finger on selected text / Chrome; 注音 Enter in address bar; pin + picker + S/M/L widths
- [O5] iOS session (Resume spec 效能優化) acknowledged v2.1.1 + releases/latest link (2026-11-18); product-page macOS link queued after its perf + close-last-tab changes land; page deploy waits for owner approval
- [O6] In-app updater path v2.0.x → v2.1.x never tested end to end

## 本次結案
- (none this update; previous closures in log.md)

## 指標
- Release recipe: `CARGO_PROFILE_RELEASE_STRIP=false SQLITE3_LIB_DIR=$(xcrun --show-sdk-path)/usr/lib ./scripts/notarize-release.sh <ver>` → `gh release create v<ver> release/v<ver>/* --notes-file docs/releases/v<ver>.md` (notes bilingual; bump package.json, tauri.conf.json, Cargo.toml)
- Local install: ditto bundle/macos/Peekabrowser.app to /Applications (project memory release-build-strip-quirk)
- Repo has no auto-merge enabled; CI publishes nothing unless workflow_dispatch with release_tag
- User data lives only in ~/Library/Application Support/com.peekabrowser.app (not bundled; verified)
- iOS prompt model reference: iOS-Peekabrowser App/Sources/Model/QueryLens.swift; macOS port src-tauri/src/prompts.rs

<!-- superseded 2026-11-19 15:16 sha 4f6f06ce802160f9 -->
# Peekabrowser macOS — v2.2.0 released (site catalogue, Ask elsewhere)

更新：2026-11-19

## 現在在做什麼
- Nothing in progress. Released v2.1.2 (three-finger ignores taps <1 s after a key: palm false-triggers while typing 注音) and v2.2.0 (site catalogue + search, URL-template sites, rail +, Ask elsewhere). Notarized; GitHub latest = v2.2.0.
- /Applications was 2.1.2 at session end; user to update in-app and check rail + is centred (`faeb938`, unseen).

## 卡住的點
- Background computer-use can't see the floating panels (bundle com.peekabrowser.sidebar, indexed as "Peekabrowser-2.0.6"); screenshots need full-screen control or the user.
- /Applications cannot be overwritten from this shell (App Management permission); install via in-app updater or DMG.

## 下一步
1. Promo materials (O7) for the iOS session's Mac page med-study-rpg.com/peekabrowser/mac/: 4–5 2x screenshots (light, +1 dark) with SAMPLE data only, no personal info (user's condition); zh+en headline + 1–2 lines per feature (⌘C⌘C, three-finger, left edge, iPad-matching UI); version/min macOS/chips/download+changelog URLs; optional 5–8 s loops. Signature WLK only. Save outside the iPad repo, send path to session local_2169ba68-9256-4e5c-ac06-dbbc2661b8a3.
2. Confirm rail "+" centred after the user updates to 2.2.0.

## 未結案事項
- [O3] Manual checks still pending: pin + picker + S/M/L widths (three-finger and 注音 Enter now checked)
- [O5] iOS session: product-page macOS link + new separate Mac page; deploy waits for owner approval
- [O7] Promo materials for the Mac page not yet made (see 下一步 1)
- [O8] Not ported from iOS: cookie sign-in detection and "fill after sign-in" pending query; prompt (lens) rows in Ask elsewhere; image queries to URL-template sites still open the home page. Reference: docs/ios-port-notes.md §2.3
- [O9] Palm-size filter for three-finger lookup: logs now record contact size (fingers ~0.8–1.2) and since_key; threshold not set

## 本次結案
- [O6] In-app updater tested: user upgraded 2.1.1 → 2.1.2 via Settings → Updates

## 指標
- Release recipe: bump package.json, src-tauri/tauri.conf.json, Cargo.toml (+ `cargo update -p peekabrowser --offline`) → `CARGO_PROFILE_RELEASE_STRIP=false SQLITE3_LIB_DIR=$(xcrun --show-sdk-path)/usr/lib ./scripts/notarize-release.sh <ver>` (sandbox off) → `gh release create v<ver> release/v<ver>/* --target main --notes-file docs/releases/v<ver>.md`
- New Tauri commands must be added in 3 places: lib.rs generate_handler, build.rs COMMANDS (array length!), permissions/app.toml local-windows; commit permissions/autogenerated/*.toml
- Catalogue: src-tauri/src/destinations/catalog.rs (port of iOS DestinationCatalog.swift, add-site-sheet branch ec9a7ab); providers.js is now a copy of iOS App/Resources/providers.js
- iOS port notes: docs/ios-port-notes.md (path:line refs into the iOS add-site-sheet worktree)
- Logs: ~/Library/Logs/Peekabrowser/peekabrowser.log

<!-- superseded 2026-11-20 08:34 sha f90d391dc7d549c0 -->
# Peekabrowser macOS — v2.2.0 released; Mac page screenshots delivered

更新：2026-11-19

## 現在在做什麼
- Nothing in progress. Mac product-page screenshots (dark + light, 2x) are in ~/claude_domain/vibe-coding/peekabrowser-mac-promo/ (README has file table, content notes, copy facts). Paths sent to iOS session local_2169ba68-9256-4e5c-ac06-dbbc2661b8a3, which builds the page and draws ⌘C⌘C / three-finger / left-edge as animations.
- /Applications is 2.2.0; rail "+" confirmed centred.

## 卡住的點
- Reshooting needs the quirks in peekabrowser-mac-promo/README.md "Reshoot recipe" (⌘C⌘C via System Events, no Escape, white backdrop for light glass).
- /Applications cannot be overwritten from this shell (App Management permission); install via in-app updater or DMG.

## 下一步
1. Answer the iOS session if it asks for more shots (same folder, update README).
2. O10: decide whether the rail should fit 8 default entries without clipping.

## 未結案事項
- [O3] Manual checks still pending: pin + picker + S/M/L widths (three-finger and 注音 Enter now checked)
- [O5] iOS session: product-page macOS link + new separate Mac page; deploy waits for owner approval
- [O8] Not ported from iOS: cookie sign-in detection and "fill after sign-in" pending query; prompt (lens) rows in Ask elsewhere; image queries to URL-template sites still open the home page. Reference: docs/ios-port-notes.md §2.3
- [O9] Palm-size filter for three-finger lookup: logs now record contact size (fingers ~0.8–1.2) and since_key; threshold not set
- [O10] Fresh-install rail (6 sites + Calendar + Reminders) at 630 pt height: Calendar label clipped, Reminders scrolled out of view (see promo 05-default-sidebar-rail*.png)

## 本次結案
- [O7] Promo materials made: 9 shot types dark + light in peekabrowser-mac-promo/, delivered to the iOS session

## 指標
- Promo assets: ~/claude_domain/vibe-coding/peekabrowser-mac-promo/README.md
- Release recipe: bump package.json, src-tauri/tauri.conf.json, Cargo.toml (+ `cargo update -p peekabrowser --offline`) → `CARGO_PROFILE_RELEASE_STRIP=false SQLITE3_LIB_DIR=$(xcrun --show-sdk-path)/usr/lib ./scripts/notarize-release.sh <ver>` (sandbox off) → `gh release create v<ver> release/v<ver>/* --target main --notes-file docs/releases/v<ver>.md`
- New Tauri commands must be added in 3 places: lib.rs generate_handler, build.rs COMMANDS (array length!), permissions/app.toml local-windows; commit permissions/autogenerated/*.toml
- Catalogue: src-tauri/src/destinations/catalog.rs; defaults: destinations/defaults.rs; picker position: panel/mod.rs show_picker (near cursor)
- iOS port notes: docs/ios-port-notes.md
- Logs: ~/Library/Logs/Peekabrowser/peekabrowser.log

<!-- superseded 2026-11-25 08:19 sha d6f39fbc597c99c0 -->
# Peekabrowser macOS — v2.2.0 released; notarization .p8 moved to keychain only

更新：2026-11-20

## 現在在做什麼
- Nothing in progress. Mac product-page screenshots (dark + light, 2x) are in ~/claude_domain/vibe-coding/peekabrowser-mac-promo/ (README has file table, content notes, copy facts). Paths sent to iOS session local_2169ba68-9256-4e5c-ac06-dbbc2661b8a3, which builds the page and draws ⌘C⌘C / three-finger / left-edge as animations.
- /Applications is 2.2.0; rail "+" confirmed centred.
- 2026-11-20: Downloads .p8 moved to Trash (verified identical to keychain copy). No code changes.

## 卡住的點
- Reshooting needs the quirks in peekabrowser-mac-promo/README.md "Reshoot recipe" (⌘C⌘C via System Events, no Escape, white backdrop for light glass).
- /Applications cannot be overwritten from this shell (App Management permission); install via in-app updater or DMG.

## 下一步
1. Answer the iOS session if it asks for more shots (same folder, update README).
2. O10: decide whether the rail should fit 8 default entries without clipping.

## 未結案事項
- [O3] Manual checks still pending: pin + picker + S/M/L widths (three-finger and 注音 Enter now checked)
- [O5] iOS session: product-page macOS link + new separate Mac page; deploy waits for owner approval
- [O8] Not ported from iOS: cookie sign-in detection and "fill after sign-in" pending query; prompt (lens) rows in Ask elsewhere; image queries to URL-template sites still open the home page. Reference: docs/ios-port-notes.md §2.3
- [O9] Palm-size filter for three-finger lookup: logs now record contact size (fingers ~0.8–1.2) and since_key; threshold not set
- [O10] Fresh-install rail (6 sites + Calendar + Reminders) at 630 pt height: Calendar label clipped, Reminders scrolled out of view (see promo 05-default-sidebar-rail*.png)
- [O11] AuthKey_QT2LL9PG8Y.p8 sits in the Trash; owner empties it once notarization is confirmed working

## 本次結案
- (none)

## 指標
- Notarization: script uses notarytool profile `peekabrowser-notary`; raw .p8 = keychain item acct QT2LL9PG8Y (base64), restore cmd in project memory notarization-key-in-keychain.md
- Promo assets: ~/claude_domain/vibe-coding/peekabrowser-mac-promo/README.md
- Release recipe: bump package.json, src-tauri/tauri.conf.json, Cargo.toml (+ `cargo update -p peekabrowser --offline`) → `CARGO_PROFILE_RELEASE_STRIP=false SQLITE3_LIB_DIR=$(xcrun --show-sdk-path)/usr/lib ./scripts/notarize-release.sh <ver>` (sandbox off) → `gh release create v<ver> release/v<ver>/* --target main --notes-file docs/releases/v<ver>.md`
- New Tauri commands must be added in 3 places: lib.rs generate_handler, build.rs COMMANDS (array length!), permissions/app.toml local-windows; commit permissions/autogenerated/*.toml
- Catalogue: src-tauri/src/destinations/catalog.rs; defaults: destinations/defaults.rs; picker position: panel/mod.rs show_picker (near cursor)
- iOS port notes: docs/ios-port-notes.md
- Logs: ~/Library/Logs/Peekabrowser/peekabrowser.log
