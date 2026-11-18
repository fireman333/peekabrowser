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
