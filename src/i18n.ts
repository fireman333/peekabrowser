// Minimal i18n: key → { zh-TW, en }. Default language is Traditional Chinese (Taiwan).
// The Rust side mirrors the strings it shows itself (tray menu, notifications) in i18n.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Lang = "zh-TW" | "en";
export const DEFAULT_LANG: Lang = "zh-TW";

type Entry = { "zh-TW": string; en: string };

const DICT = {
  // ─── Common ───
  "common.save": { "zh-TW": "儲存", en: "Save" },
  "common.cancel": { "zh-TW": "取消", en: "Cancel" },
  "common.update": { "zh-TW": "更新", en: "Update" },
  "common.create": { "zh-TW": "建立", en: "Create" },

  // ─── Settings: page ───
  "settings.title": { "zh-TW": "設定", en: "Settings" },

  // Destinations
  "dest.custom": { "zh-TW": "＋ 自訂", en: "+ Custom" },
  "dest.name": { "zh-TW": "名稱", en: "Name" },
  "dest.namePh": { "zh-TW": "輸入名稱", en: "Enter name" },
  "dest.url": { "zh-TW": "網址", en: "URL" },
  "dest.urlPh": { "zh-TW": "貼上網址", en: "Paste URL" },
  "dest.icon": { "zh-TW": "圖示（emoji）", en: "Icon (emoji)" },
  "dest.iconPh": { "zh-TW": "留空則使用網站圖示", en: "Leave empty to use the website icon" },
  "dest.prompt": { "zh-TW": "提示詞前綴", en: "Prompt prefix" },
  "dest.promptPh": { "zh-TW": "例：請翻譯以下文字為英文：", en: "e.g. Translate the following into English:" },
  "dest.moveUp": { "zh-TW": "上移", en: "Move up" },
  "dest.moveDown": { "zh-TW": "下移", en: "Move down" },
  "dest.remove": { "zh-TW": "移除", en: "Remove" },
  "dest.required": { "zh-TW": "名稱與網址為必填", en: "Name and URL are required" },
  "dest.saveFailed": { "zh-TW": "儲存目的地失敗：", en: "Failed to save destination: " },
  "dest.empty": { "zh-TW": "尚未設定任何目的地", en: "No destinations yet" },

  // ⌘C ⌘C
  "copy.axOpen": { "zh-TW": "重新授權", en: "Re-grant" },

  // Shortcuts
  "keys.toggle": { "zh-TW": "顯示／隱藏側邊欄", en: "Toggle sidebar" },
  "keys.screenshot": { "zh-TW": "截圖傳給 AI", en: "Screenshot to AI" },
  "keys.export": { "zh-TW": "儲存目前頁面的回答", en: "Save answer from the current page" },
  "keys.copy": { "zh-TW": "快速傳送剪貼簿", en: "Quick send clipboard" },
  "keys.reload": { "zh-TW": "重新載入目前頁面", en: "Reload active page" },
  "keys.close": { "zh-TW": "關閉目前分頁", en: "Close active tab" },
  "keys.recording": { "zh-TW": "請按下按鍵…", en: "Press keys…" },
  "keys.saveFailed": { "zh-TW": "儲存快捷鍵失敗：", en: "Failed to save shortcut: " },

  // Updates
  "upd.section": { "zh-TW": "更新", en: "Updates" },
  "upd.check": { "zh-TW": "檢查更新", en: "Check for updates" },
  "upd.install": { "zh-TW": "安裝並重新啟動", en: "Install and restart" },
  "upd.notes": { "zh-TW": "這個版本的新功能", en: "What's new in this version" },
  "upd.notChecked": { "zh-TW": "尚未檢查", en: "Not checked yet" },
  "upd.autoCheck": { "zh-TW": "自動檢查更新", en: "Check for updates automatically" },
  "upd.autoInstall": { "zh-TW": "自動安裝更新", en: "Install updates automatically" },
  "upd.autoInstallHelp": {
    "zh-TW": "找到新版本時自動下載、驗證並重新啟動（只在側邊欄隱藏且沒有正在生成的回答時進行）。關閉時只會通知。",
    en: "Downloads, verifies and restarts automatically when a new version is found (only while the sidebar is hidden and nothing is generating). Off: notify only.",
  },
  "upd.manualPrompt": { "zh-TW": "要改到下載頁面手動更新嗎？", en: "Open the download page to update manually?" },

  // General
  "gen.language": { "zh-TW": "介面語言", en: "Language" },
  "gen.autostart": { "zh-TW": "登入時自動啟動", en: "Launch at login" },
  "gen.autostartFailed": { "zh-TW": "無法更改登入時自動啟動：", en: "Couldn't change Launch at login: " },

  // --- settings (settings agent) ---
  "s.sec.lookup": { "zh-TW": "查詢時", en: "When you look something up" },
  "s.sec.sites": { "zh-TW": "你的 AI 與搜尋網站", en: "Your AIs and search sites" },
  "s.sec.other": { "zh-TW": "從其他 app 使用", en: "Use it from other apps" },
  "s.sec.general": { "zh-TW": "一般", en: "General" },
  "s.sec.about": { "zh-TW": "關於", en: "About" },
  "s.fromApps": { "zh-TW": "來自其他 app 的文字", en: "Text from other apps" },
  "s.mode.last": { "zh-TW": "上次用的", en: "Last used" },
  "s.mode.fixed": { "zh-TW": "固定一個", en: "Fixed one" },
  "s.mode.choose": { "zh-TW": "讓我選", en: "Let me choose" },
  "s.sendTo": { "zh-TW": "傳送到", en: "Send to" },
  "s.fromApps.foot": { "zh-TW": "⌘C ⌘C 與三指輕點會照這裡的設定傳送。", en: "⌘C ⌘C and the three-finger tap follow this setting." },
  "s.autoSubmit": { "zh-TW": "自動送出", en: "Auto-submit" },
  "s.autoSubmit.foot": { "zh-TW": "開：填入並送出。關：填入後由你按送出。", en: "On: fills in and sends. Off: fills in and waits for you to press send." },
  "s.threeFinger": { "zh-TW": "三指輕點查詢", en: "Three-finger tap to look up" },
  "s.threeFinger.foot": {
    "zh-TW": "用三指輕點觸控板，查詢游標下的文字。若不想同時跳出系統字典，請到「系統設定 → 觸控板 → 查詢與資料偵測器」關閉。",
    en: "Tap the trackpad with three fingers to look up the text under the cursor. To stop the system dictionary from popping up too, turn off Look up & data detectors in System Settings → Trackpad.",
  },
  "s.openTrackpad": { "zh-TW": "打開觸控板設定", en: "Open Trackpad settings" },
  "s.axTitle": { "zh-TW": "需要「輔助使用」權限", en: "Accessibility permission needed" },
  "s.axHelp": {
    "zh-TW": "⌘C ⌘C 與三指輕點需要它才能運作。按「重新授權」後，在系統設定裡打開 Peekabrowser 的開關；授權後會自動啟用。",
    en: "⌘C ⌘C and the three-finger tap need it. Click Re-grant, then switch Peekabrowser on in System Settings; it turns on automatically once granted.",
  },
  "s.reorder": { "zh-TW": "重新排序", en: "Reorder" },
  "s.done": { "zh-TW": "完成", en: "Done" },
  "s.default": { "zh-TW": "預設", en: "Default" },
  "s.addSite": { "zh-TW": "＋ 新增網站", en: "+ Add a site" },
  "s.sites.foot": { "zh-TW": "第一個是預設。", en: "The first one is the default." },
  "s.back": { "zh-TW": "‹ 設定", en: "‹ Settings" },
  "s.site.addTitle": { "zh-TW": "新增網站", en: "Add a site" },
  "s.site.editTitle": { "zh-TW": "編輯網站", en: "Edit site" },
  "s.site.catalog": { "zh-TW": "常用網站", en: "Popular sites" },
  "s.site.promptHelp": { "zh-TW": "會直接放在你送出的文字前面。", en: "Goes right in front of the text you send." },
  "s.site.delete": { "zh-TW": "刪除這個網站", en: "Delete this site" },
  "s.edge": { "zh-TW": "滑鼠移到螢幕左緣時顯示側邊欄", en: "Show the sidebar at the left screen edge" },
  "s.shortcuts": { "zh-TW": "全域快捷鍵", en: "Global shortcuts" },
  "s.shortcuts.foot": { "zh-TW": "點一下快捷鍵，再按新的組合；按 Esc 取消。", en: "Click a shortcut, then press a new combination. Esc cancels." },
  "s.fixedKeys": { "zh-TW": "固定的快捷鍵", en: "Fixed shortcuts" },
  "s.newTab": { "zh-TW": "開新分頁", en: "New tab" },
  "s.reopenTab": { "zh-TW": "重新開啟剛關閉的分頁", en: "Reopen closed tab" },
  "s.github": { "zh-TW": "GitHub 發行頁", en: "GitHub releases" },
  "s.license": { "zh-TW": "授權", en: "License" },
  "s.signature": { "zh-TW": "Peekabrowser {v} · WLK", en: "Peekabrowser {v} · WLK" },
  "picker.defaultTag": { "zh-TW": "預設", en: "Default" },

  // ─── Picker ───
  "picker.sendTo":{ "zh-TW": "傳送到…", en: "Send to…" },
  "picker.sendText": { "zh-TW": "傳送文字到…", en: "Send text to…" },
  "picker.sendShot": { "zh-TW": "傳送截圖到…", en: "Send screenshot to…" },
  "picker.dismiss": { "zh-TW": "關閉（Esc）", en: "Dismiss (Esc)" },
  "picker.listLabel": { "zh-TW": "目的地", en: "Destinations" },
  "picker.hint": { "zh-TW": "C V B N M 快速選擇 · Esc 關閉", en: "C V B N M to pick · Esc to close" },
  "picker.shotAlt": { "zh-TW": "截圖預覽", en: "Screenshot preview" },
  "picker.prompt": { "zh-TW": "提示詞", en: "prompt" },

  // ─── Sidebar ───
  "side.save": { "zh-TW": "儲存回答（⌘⇧E）", en: "Save answer (⌘⇧E)" },
  "side.back": { "zh-TW": "上一頁（⌘[）", en: "Back (⌘[)" },
  "side.forward": { "zh-TW": "下一頁（⌘]）", en: "Forward (⌘])" },
  "side.reload": { "zh-TW": "重新載入（⌘R）", en: "Reload (⌘R)" },
  "side.settings": { "zh-TW": "設定", en: "Settings" },
  "side.settingsUpdate": { "zh-TW": "設定 — 有新版本 {v}", en: "Settings — version {v} available" },
  "side.savedPartial": { "zh-TW": "已儲存（仍在生成中）", en: "Saved (still generating)" },
  "side.saved": { "zh-TW": "已儲存回答", en: "Answer saved" },

  // ─── Records ───
  "rec.searchPh": { "zh-TW": "搜尋原文、提示詞、回答、筆記…", en: "Search text, prompts, answers, notes…" },
  "rec.searchLabel": { "zh-TW": "搜尋紀錄", en: "Search records" },
  "rec.favOnly": { "zh-TW": "只顯示收藏", en: "Favorites only" },
  "rec.listLabel": { "zh-TW": "紀錄", en: "Records" },
  "rec.unavailable": { "zh-TW": "無法讀取紀錄：{e}", en: "Records are unavailable: {e}" },
  "rec.screenshot": { "zh-TW": "截圖", en: "Screenshot" },
  "rec.answerOf": { "zh-TW": "{name} 的回答", en: "{name} answer" },
  "rec.noMatch": { "zh-TW": "沒有符合的紀錄", en: "No matching records" },
  "rec.none": { "zh-TW": "還沒有紀錄", en: "No records yet" },
  "rec.emptyTitle": { "zh-TW": "閱讀時的提問都會收在這裡", en: "Your reading questions live here" },
  "rec.emptyP1": {
    "zh-TW": "在任何地方選取文字後連按兩次 <kbd>⌘C</kbd>，或用 <kbd>⌘⇧S</kbd> 截取區域，再選擇目的地。",
    en: "Select text anywhere and press <kbd>⌘C</kbd> twice, or capture an area with <kbd>⌘⇧S</kbd>, then pick a destination.",
  },
  "rec.emptyP2": {
    "zh-TW": "覺得回答有用時，按 <kbd>⌘⇧E</kbd>（或側邊欄的儲存按鈕）就會連同原文一起保存在這裡。",
    en: "When the answer is useful, press <kbd>⌘⇧E</kbd> (or the save button in the sidebar) to keep it here with the original text.",
  },
  "rec.from": { "zh-TW": "來自 {app}", en: "from {app}" },
  "rec.favorite": { "zh-TW": "收藏", en: "Favorite" },
  "rec.reopen": { "zh-TW": "重新開啟", en: "Reopen" },
  "rec.reopenTitle": { "zh-TW": "重新開啟對話", en: "Reopen conversation" },
  "rec.copyMd": { "zh-TW": "複製 MD", en: "Copy MD" },
  "rec.copyMdTitle": { "zh-TW": "複製為 Markdown", en: "Copy as Markdown" },
  "rec.copied": { "zh-TW": "已複製", en: "Copied" },
  "rec.export": { "zh-TW": "匯出 .md", en: "Export .md" },
  "rec.exportTitle": { "zh-TW": "匯出 Markdown 到「下載」資料夾", en: "Export Markdown to Downloads" },
  "rec.delete": { "zh-TW": "刪除", en: "Delete" },
  "rec.deleteTitle": { "zh-TW": "刪除紀錄", en: "Delete record" },
  "rec.deleteConfirm": { "zh-TW": "要刪除這筆紀錄與截圖嗎？此動作無法復原。", en: "Delete this record and its screenshot? This can't be undone." },
  "rec.original": { "zh-TW": "原文", en: "Original" },
  "rec.loadingShot": { "zh-TW": "正在載入截圖…", en: "Loading screenshot…" },
  "rec.notRecorded": { "zh-TW": "未記錄（回答為手動儲存）。", en: "Not recorded (answer saved manually)." },
  "rec.prompt": { "zh-TW": "提示詞", en: "Prompt" },
  "rec.answer": { "zh-TW": "回答", en: "Answer" },
  "rec.notSaved": { "zh-TW": "尚未儲存。開啟對話後按 ⌘⇧E 儲存。", en: "Not saved yet. Open the conversation and press ⌘⇧E to save it." },
  "rec.links": { "zh-TW": "連結", en: "Links" },
  "rec.conversation": { "zh-TW": "對話：", en: "Conversation: " },
  "rec.tags": { "zh-TW": "標籤", en: "Tags" },
  "rec.tagsPh": { "zh-TW": "以逗號分隔", en: "comma, separated" },
  "rec.note": { "zh-TW": "筆記", en: "Note" },
  "rec.notePh": { "zh-TW": "你的筆記", en: "Your note" },
  "rec.shotAlt": { "zh-TW": "擷取的截圖", en: "Captured screenshot" },
  "rec.shotMissing": { "zh-TW": "找不到截圖檔案。", en: "Screenshot file missing." },
  "rec.capComplete": { "zh-TW": "完整回答", en: "Complete answer" },
  "rec.capPartial": { "zh-TW": "部分 — 生成途中儲存", en: "Partial — saved while generating" },
  "rec.capUnknown": { "zh-TW": "已儲存；此網站無法判斷是否完整", en: "Saved; completeness unknown on this site" },
  "rec.capSelection": { "zh-TW": "已儲存選取內容", en: "Saved selection" },
  "rec.stSaved": { "zh-TW": "已儲存", en: "saved" },
  "rec.stQueued": { "zh-TW": "排隊中", en: "queued" },
  "rec.stDraft": { "zh-TW": "已放入（未送出）", en: "placed, not sent" },
  "rec.stInjected": { "zh-TW": "已送出", en: "injected" },
  "rec.stGenerating": { "zh-TW": "生成中", en: "generating" },
  "rec.stCompleted": { "zh-TW": "已完成", en: "completed" },
  "rec.stFailed": { "zh-TW": "失敗", en: "failed" },

  // ─── System config (Calendar / Reminders quick create) ───
  "sys.newEvent": { "zh-TW": "新增行事曆事件", en: "New Calendar Event" },
  "sys.newReminder": { "zh-TW": "新增提醒事項", en: "New Reminder" },
  "sys.title": { "zh-TW": "標題", en: "Title" },
  "sys.titlePh": { "zh-TW": "事件標題…", en: "Event title…" },
  "sys.calendar": { "zh-TW": "行事曆", en: "Calendar" },
  "sys.list": { "zh-TW": "列表", en: "List" },
  "sys.start": { "zh-TW": "開始", en: "Start" },
  "sys.end": { "zh-TW": "結束", en: "End" },
  "sys.due": { "zh-TW": "截止日期（選填）", en: "Due Date (optional)" },
  "sys.ocrRunning": { "zh-TW": "正在辨識文字（OCR）…", en: "Running OCR…" },
  "sys.ocrEmpty": { "zh-TW": "OCR 沒有辨識到文字 — 請手動輸入", en: "OCR returned empty — type manually" },
  "sys.ocrFailed": { "zh-TW": "OCR 失敗：{e} — 請手動輸入", en: "OCR failed: {e} — type manually" },
  "sys.creating": { "zh-TW": "建立中…", en: "Creating…" },
  "sys.failed": { "zh-TW": "建立失敗：", en: "Failed: " },

  // --- chrome (layout agent) ---
  "rail.label": { "zh-TW": "目的地", en: "Destinations" },
  "rail.footer": { "zh-TW": "紀錄、釘選與設定", en: "Records, pin and settings" },
  "rail.records": { "zh-TW": "紀錄", en: "Records" },
  "rail.pin": { "zh-TW": "釘選視窗（不自動隱藏）", en: "Pin window (don't auto-hide)" },
  "rail.unpin": { "zh-TW": "取消釘選視窗", en: "Unpin window" },
  "rail.tabs": { "zh-TW": "{name}，{n} 個分頁", en: "{name}, {n} tabs" },
  "rail.generating": { "zh-TW": "正在生成回答", en: "writing an answer" },
  "rail.unseen": { "zh-TW": "有還沒看的新回答", en: "new answer not seen yet" },
  "chrome.toolbar": { "zh-TW": "網頁工具列", en: "Page toolbar" },
  "chrome.address": { "zh-TW": "網址（點一下編輯，⌘L）", en: "Address (click to edit, ⌘L)" },
  "chrome.addressPh": { "zh-TW": "輸入網址或要搜尋的字", en: "Type a URL or search words" },
  "chrome.secure": { "zh-TW": "安全連線", en: "Secure connection" },
  "chrome.loading": { "zh-TW": "載入中", en: "Loading" },
  "chrome.noPage": { "zh-TW": "沒有開啟中的頁面", en: "No page open" },
  "chrome.nav": { "zh-TW": "瀏覽動作", en: "Page actions" },
  "chrome.openBrowser": { "zh-TW": "在預設瀏覽器開啟", en: "Open in default browser" },
  "chrome.more": { "zh-TW": "更多：截圖、寬度、重新開啟分頁、複製網址", en: "More: capture, width, reopen tab, copy URL" },
  "chrome.newTab": { "zh-TW": "新分頁（⌘T）", en: "New tab (⌘T)" },
  "chrome.newTabFull": { "zh-TW": "這個目的地已達 20 個分頁", en: "This destination has 20 tabs" },
  "chrome.tabs": { "zh-TW": "分頁", en: "Tabs" },
  "chrome.closeTab": { "zh-TW": "關閉分頁（⌘W）", en: "Close tab (⌘W)" },
  "chrome.closeNamed": { "zh-TW": "關閉「{name}」", en: "Close “{name}”" },
  "chrome.unloaded": { "zh-TW": "已卸載，點一下重新載入", en: "unloaded, click to reload" },
} satisfies Record<string, Entry>;

export type Key = keyof typeof DICT;

let lang: Lang = DEFAULT_LANG;
const listeners: Array<(l: Lang) => void> = [];

export function normalizeLang(v: unknown): Lang {
  return v === "en" ? "en" : "zh-TW";
}

export function getLang(): Lang {
  return lang;
}

/** Translate `key`, replacing `{name}` placeholders from `vars`. */
export function t(key: Key, vars?: Record<string, string | number>): string {
  const entry = DICT[key] as Entry | undefined;
  let s = entry ? entry[lang] : String(key);
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(String(v));
  return s;
}

/**
 * Fill static markup: `data-i18n` (textContent), `data-i18n-html` (trusted innerHTML),
 * `data-i18n-title`, `data-i18n-aria` (aria-label), `data-i18n-placeholder`.
 */
export function applyI18n(root: ParentNode = document): void {
  document.documentElement.lang = lang === "en" ? "en" : "zh-Hant-TW";
  root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => (el.textContent = t(el.dataset.i18n as Key)));
  root.querySelectorAll<HTMLElement>("[data-i18n-html]").forEach((el) => (el.innerHTML = t(el.dataset.i18nHtml as Key)));
  root.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((el) => (el.title = t(el.dataset.i18nTitle as Key)));
  root.querySelectorAll<HTMLElement>("[data-i18n-aria]").forEach((el) => el.setAttribute("aria-label", t(el.dataset.i18nAria as Key)));
  root
    .querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("[data-i18n-placeholder]")
    .forEach((el) => (el.placeholder = t(el.dataset.i18nPlaceholder as Key)));
}

/** Register a re-render callback for language changes. */
export function onLangChange(fn: (l: Lang) => void): void {
  listeners.push(fn);
}

export function setLang(next: Lang): void {
  if (next === lang) return;
  lang = next;
  applyI18n();
  listeners.forEach((fn) => fn(lang));
}

/** Load the saved language, apply it, and follow changes made in Settings. */
export async function initI18n(): Promise<void> {
  // Never let a failed or slow settings read block a page from rendering:
  // fall back to the default language after a short timeout.
  let next: Lang = DEFAULT_LANG;
  try {
    const s = await Promise.race([
      invoke<{ language?: string }>("get_app_settings"),
      new Promise<null>((resolve) => setTimeout(() => resolve(null), 1500)),
    ]);
    if (s) next = normalizeLang(s.language);
  } catch (e) {
    console.warn("i18n: falling back to default language", e);
  }
  const changed = next !== lang;
  lang = next;
  try {
    applyI18n();
    if (changed) listeners.forEach((fn) => fn(lang));
  } catch (e) {
    console.error("i18n: apply failed", e);
  }
  try {
    listen<string>("language-changed", (e) => setLang(normalizeLang(e.payload))).catch(() => {});
  } catch {
    /* not running inside Tauri */
  }
}
