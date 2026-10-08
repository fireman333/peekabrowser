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
  "settings.windowTitle": { "zh-TW": "Peekabrowser 設定", en: "Peekabrowser Settings" },
  "settings.title": { "zh-TW": "設定", en: "Settings" },

  // Destinations
  "dest.section": { "zh-TW": "目的地", en: "Destinations" },
  "dest.hint": {
    "zh-TW": "管理搜尋引擎與 AI 目的地。用 ↑ ↓ 調整順序；第一個目的地會用在 ⌘C ⌘C 的預設傳送。",
    en: "Manage your search engines and AI destinations. Use ↑ ↓ to reorder; the first one is the ⌘C ⌘C default.",
  },
  "dest.add": { "zh-TW": "＋ 新增目的地", en: "+ Add Destination" },
  "dest.addTitle": { "zh-TW": "新增目的地", en: "Add Destination" },
  "dest.editTitle": { "zh-TW": "編輯目的地", en: "Edit Destination" },
  "dest.presets": { "zh-TW": "快速選擇", en: "Presets" },
  "dest.custom": { "zh-TW": "＋ 自訂", en: "+ Custom" },
  "dest.name": { "zh-TW": "名稱", en: "Name" },
  "dest.namePh": { "zh-TW": "輸入名稱", en: "Enter name" },
  "dest.url": { "zh-TW": "網址", en: "URL" },
  "dest.urlPh": { "zh-TW": "貼上網址", en: "Paste URL" },
  "dest.icon": { "zh-TW": "圖示（emoji）", en: "Icon (emoji)" },
  "dest.iconPh": { "zh-TW": "留空則使用網站圖示", en: "Leave empty to use the website icon" },
  "dest.prompt": { "zh-TW": "提示詞前綴", en: "Prompt prefix" },
  "dest.promptHelp": { "zh-TW": "自動加在傳送的文字前面", en: "Added in front of the text you send" },
  "dest.promptPh": { "zh-TW": "例：請翻譯以下文字為英文：", en: "e.g. Translate the following into English:" },
  "dest.moveUp": { "zh-TW": "上移", en: "Move up" },
  "dest.moveDown": { "zh-TW": "下移", en: "Move down" },
  "dest.edit": { "zh-TW": "編輯", en: "Edit" },
  "dest.remove": { "zh-TW": "移除", en: "Remove" },
  "dest.required": { "zh-TW": "名稱與網址為必填", en: "Name and URL are required" },
  "dest.saveFailed": { "zh-TW": "儲存目的地失敗：", en: "Failed to save destination: " },
  "dest.empty": { "zh-TW": "尚未設定任何目的地", en: "No destinations yet" },

  // ⌘C ⌘C
  "copy.section": { "zh-TW": "⌘C ⌘C 快速傳送", en: "⌘C ⌘C Quick send" },
  "copy.autoFirst": { "zh-TW": "直接傳送到第一個目的地", en: "Send straight to the first destination" },
  "copy.autoFirstHelp": {
    "zh-TW": "開啟後，⌘C ⌘C 直接傳送到清單中的第一個目的地，不再顯示選單。關閉時，可在選單中按 C / V / B / N / M 快速選擇前五個（注音等輸入法下也可用）。",
    en: "On: ⌘C ⌘C sends straight to the first destination without the menu. Off: press C / V / B / N / M in the menu to pick one of the first five (works with any input method).",
  },

  // Appearance & power
  "power.section": { "zh-TW": "外觀與省電", en: "Appearance & power" },
  "power.edge": { "zh-TW": "滑鼠移到螢幕左緣時顯示側邊欄", en: "Show sidebar at the left screen edge" },
  "power.edgeHelp": {
    "zh-TW": "關閉後，側邊欄隱藏時不會監聽全域滑鼠位置；⌘⇧A 與選單列圖示仍可使用。",
    en: "Off: no global mouse monitoring while hidden. ⌘⇧A and the menu-bar icon still work.",
  },
  "power.material": { "zh-TW": "原生視窗材質", en: "Native window material" },
  "power.materialHelp": {
    "zh-TW": "macOS 26 以上使用 Liquid Glass，macOS 13–15 使用毛玻璃效果；關閉則為純色。重新啟動後生效。",
    en: "Liquid Glass on macOS 26+, vibrancy on macOS 13–15. Off: solid colors. Applies after restart.",
  },
  "power.unload": { "zh-TW": "背景頁面閒置多久後卸載", en: "Unload hidden pages after" },
  "power.unloadHelp": {
    "zh-TW": "卸載的頁面會保留標題與網址，點一下即可重新載入。正在生成回答或有未送出草稿的頁面不會被卸載。",
    en: "Unloaded pages keep their title and URL and reload when clicked. Pages that are generating or have an unsent draft are kept.",
  },
  "power.min": { "zh-TW": "{n} 分鐘", en: "{n} min" },
  "diag.status": { "zh-TW": "狀態：已載入 {loaded}/{total} 個頁面", en: "Status: {loaded}/{total} pages loaded" },
  "diag.activity": { "zh-TW": "{n} 個回答生成中（暫不休眠）", en: "{n} generation(s) keeping the app awake" },
  "diag.noActivity": { "zh-TW": "目前無背景活動", en: "no activity token held" },
  "diag.material": { "zh-TW": "材質：{m}", en: "material: {m}" },
  "diag.vibrancy": { "zh-TW": "毛玻璃", en: "vibrancy" },
  "diag.keyTriggered": { "zh-TW": "⌘C⌘C：按鍵觸發", en: "⌘C⌘C: key-triggered" },
  "diag.sampling": {
    "zh-TW": "⌘C⌘C：自適應偵測（授予「輔助使用」權限可改為按鍵觸發）",
    en: "⌘C⌘C: adaptive sampling (grant Accessibility for key-triggered)",
  },

  // Shortcuts
  "keys.section": { "zh-TW": "快捷鍵", en: "Keyboard shortcuts" },
  "keys.hint": { "zh-TW": "點一下快捷鍵，再按下新的按鍵組合即可更改；按 Esc 取消。", en: "Click a shortcut, then press a new key combination. Esc cancels." },
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
  "upd.autoCheckHelp": { "zh-TW": "啟動時與每天檢查一次 GitHub Releases。", en: "Checks GitHub Releases at launch and once a day." },
  "upd.autoInstall": { "zh-TW": "自動安裝更新", en: "Install updates automatically" },
  "upd.autoInstallHelp": {
    "zh-TW": "找到新版本時自動下載、驗證並重新啟動（只在側邊欄隱藏且沒有正在生成的回答時進行）。關閉時只會通知。",
    en: "Downloads, verifies and restarts automatically when a new version is found (only while the sidebar is hidden and nothing is generating). Off: notify only.",
  },
  "upd.manualPrompt": { "zh-TW": "要改到下載頁面手動更新嗎？", en: "Open the download page to update manually?" },

  // General
  "gen.section": { "zh-TW": "一般", en: "General" },
  "gen.language": { "zh-TW": "介面語言", en: "Language" },
  "gen.languageHelp": { "zh-TW": "套用到所有視窗、選單列與通知。", en: "Applies to all windows, the menu bar and notifications." },
  "gen.autostart": { "zh-TW": "登入時自動啟動", en: "Launch at login" },
  "gen.autostartHelp": {
    "zh-TW": "登入 macOS 後自動在背景開啟 Peekabrowser。狀態直接讀取系統設定。",
    en: "Open Peekabrowser in the background when you log in to macOS. Reflects the actual system state.",
  },
  "gen.autostartFailed": { "zh-TW": "無法更改登入時自動啟動：", en: "Couldn't change Launch at login: " },

  // ─── Picker ───
  "picker.sendTo": { "zh-TW": "傳送到…", en: "Send to…" },
  "picker.sendText": { "zh-TW": "傳送文字到…", en: "Send text to…" },
  "picker.sendShot": { "zh-TW": "傳送截圖到…", en: "Send screenshot to…" },
  "picker.dismiss": { "zh-TW": "關閉（Esc）", en: "Dismiss (Esc)" },
  "picker.listLabel": { "zh-TW": "目的地", en: "Destinations" },
  "picker.hint": { "zh-TW": "C V B N M 快速選擇 · Esc 關閉", en: "C V B N M to pick · Esc to close" },
  "picker.shotAlt": { "zh-TW": "截圖預覽", en: "Screenshot preview" },
  "picker.prompt": { "zh-TW": "提示詞", en: "prompt" },

  // ─── Sidebar ───
  "side.navLabel": { "zh-TW": "目的地與頁面", en: "Destinations and pages" },
  "side.answerGroup": { "zh-TW": "回答", en: "Answer" },
  "side.save": { "zh-TW": "儲存回答（⌘⇧E）", en: "Save answer (⌘⇧E)" },
  "side.records": { "zh-TW": "紀錄", en: "Records" },
  "side.navGroup": { "zh-TW": "瀏覽", en: "Navigation" },
  "side.back": { "zh-TW": "上一頁（⌘[）", en: "Back (⌘[)" },
  "side.forward": { "zh-TW": "下一頁（⌘]）", en: "Forward (⌘])" },
  "side.reload": { "zh-TW": "重新載入（⌘R）", en: "Reload (⌘R)" },
  "side.moreGroup": { "zh-TW": "更多動作", en: "More actions" },
  "side.newTab": { "zh-TW": "新分頁（⌘N）", en: "New tab (⌘N)" },
  "side.capture": { "zh-TW": "截圖傳給 AI（⌘⇧S）", en: "Screenshot to AI (⌘⇧S)" },
  "side.openBrowser": { "zh-TW": "在瀏覽器開啟", en: "Open in browser" },
  "side.panelSize": { "zh-TW": "面板寬度", en: "Panel size" },
  "side.narrow": { "zh-TW": "窄", en: "Narrow" },
  "side.medium": { "zh-TW": "中", en: "Medium" },
  "side.wide": { "zh-TW": "寬", en: "Wide" },
  "side.narrowShort": { "zh-TW": "窄", en: "S" },
  "side.mediumShort": { "zh-TW": "中", en: "M" },
  "side.wideShort": { "zh-TW": "寬", en: "L" },
  "side.windowGroup": { "zh-TW": "視窗", en: "Window" },
  "side.more": { "zh-TW": "更多", en: "More" },
  "side.pin": { "zh-TW": "釘選（保持開啟）", en: "Pin (keep open)" },
  "side.settings": { "zh-TW": "設定", en: "Settings" },
  "side.settingsUpdate": { "zh-TW": "設定 — 有新版本 {v}", en: "Settings — version {v} available" },
  "side.pages": { "zh-TW": "{name}，{n} 個頁面", en: "{name}, {n} page(s)" },
  "side.pagesGroup": { "zh-TW": "{name} 的頁面", en: "{name} pages" },
  "side.closePage": { "zh-TW": "關閉頁面", en: "Close page" },
  "side.closeNamed": { "zh-TW": "關閉 {name}", en: "Close {name}" },
  "side.stGenerating": { "zh-TW": "生成中", en: "generating" },
  "side.stUnloaded": { "zh-TW": "已卸載 — 點一下重新載入", en: "unloaded — click to restore" },
  "side.stCurrent": { "zh-TW": "目前頁面", en: "current" },
  "side.stIdle": { "zh-TW": "閒置", en: "idle" },
  "side.savedPartial": { "zh-TW": "已儲存（仍在生成中）", en: "Saved (still generating)" },
  "side.saved": { "zh-TW": "已儲存回答", en: "Answer saved" },

  // ─── Records ───
  "rec.windowTitle": { "zh-TW": "Peekabrowser 紀錄", en: "Peekabrowser Records" },
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
  "rec.stInjected": { "zh-TW": "已送出", en: "injected" },
  "rec.stGenerating": { "zh-TW": "生成中", en: "generating" },
  "rec.stCompleted": { "zh-TW": "已完成", en: "completed" },
  "rec.stFailed": { "zh-TW": "失敗", en: "failed" },

  // ─── System config (Calendar / Reminders quick create) ───
  "sys.windowTitle": { "zh-TW": "快速建立", en: "Quick Create" },
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
