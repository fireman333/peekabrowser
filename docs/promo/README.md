# Peekabrowser for Mac — page assets (2026-11-19, app v2.2.0)

All PNGs are 2x Retina captures of the real app, no personal data. Dark set has no suffix; `-light` files are the same shots in light appearance (01, 04-2, 04-3 are dark only).
Window captures include the macOS shadow on a transparent background.

| File | Shows |
|---|---|
| 01-pubmed-panel.png | Left-edge rail + floating panel on the PubMed home page (transparent bg, composited rail/toolbar/page windows) |
| 02-add-site-catalogue.png | Add-site catalogue filtered by "med" (academic sites, login/fill badges, custom-URL chips) |
| 03-settings.png | Settings: text from other apps (⌘C⌘C / three-finger), auto-send, prompt, three-finger toggle, your sites |
| 04-default-sites-1-search.png | Catalogue, unfiltered: general search section |
| 04-default-sites-2-ai.png | Catalogue: AI chat section |
| 04-default-sites-3-academic.png | Catalogue: academic & medical section |
| 05-default-sidebar-rail.png | Sidebar rail as preloaded on a fresh install: Google, ChatGPT, Claude, Gemini, Perplexity, OpenEvidence, +, Calendar (Reminders is scrolled out of view at this rail height) |
| 06-default-sites-settings.png | Settings → your sites, fresh-install list: the 6 sites + Calendar + Reminders, Google marked default |
| 07-cmdcc-picker.png | ⌘C⌘C: "metformin" selected in a sample note (TextEdit), site picker floating beside it, not over it. Default site set |
| 08-lookup-panel.png | After picking PubMed: panel on the left with the query results, sample note on the right, no overlap |
| 09-ask-elsewhere.png | Same query tab with the Ask elsewhere menu open (到別處問：metformin → Google / OpenEvidence) |

## Content notes
- Web content shown is PubMed only (US government site). In 08/09 the first result's authors and abstract snippet are blurred; only the article title stays readable.
- The sample note in 07/08 was written for these shots (no third-party text).
- Site icons in the catalogue are the sites' own favicons (trademarks of their owners), shown only to identify supported sites. If you'd rather avoid them, crop to the text rows or use 01/03.
- 05–07 were taken with the app temporarily on its built-in default site list; 08–09 use the owner's list (Google, OpenEvidence, PubMed).
- Light glass surfaces (rail, picker, menu) were captured over a plain white backdrop so they read as light.
- Not captured: three-finger tap (can't be simulated), video loops.

## Facts for the copy
- Version 2.2.0, macOS 13+, Apple silicon (aarch64 DMG) and Intel (x64 DMG), download: GitHub latest release.
- Default sites: Google, ChatGPT, Claude, Gemini, Perplexity, OpenEvidence + Calendar, Reminders.
- Catalogue: 63 built-in sites in General search / AI chat / Academic & medical / Library / Reference / Daily, plus custom URL templates (Claude Project, ChatGPT GPT, Gemini Gem, Perplexity Space).
- Signature: WLK.

## Reshoot recipe (quirks found 2026-11-19)
- Background computer-use can't see the floating panels; use full-screen control.
- ⌘C⌘C from computer use is not detected. Activate the source app, then `osascript` System Events `key code 8 using command down` twice (0.15 s apart) works.
- Never send Escape via System Events: it lands in the Claude window and interrupts the turn.
- Light glass windows (rail, picker, menu) render grey over a dark backdrop, and `screencapture -l` drops the backdrop. Put a white window behind, capture with `screencapture -R`, mask with the `-l` alpha.
- Release full control before capturing: its edge glow tints the screen.
- Fresh-install sidebar shows only when `~/Library/Application Support/com.peekabrowser.app/destinations.json` is absent: quit app, move it aside, relaunch, restore afterwards.
- The native Ask elsewhere menu closes when full control is released; capture just its rectangle while holding control.
