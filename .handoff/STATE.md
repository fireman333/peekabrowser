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
