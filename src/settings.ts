import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { escapeHtml } from "./icons";
import { applyI18n, getLang, initI18n, normalizeLang, onLangChange, setLang, t, type Lang } from "./i18n";

interface Destination {
  id: string;
  name: string;
  url: string;
  icon: string;
  order: number;
  clip_prompt: string;
}

interface ShortcutConfig {
  toggle_sidebar: string;
  screenshot: string;
  export: string;
}

const PRESETS = [
  { name: "Google", url: "https://www.google.com", icon: "" },
  { name: "ChatGPT", url: "https://chat.openai.com", icon: "" },
  { name: "Claude", url: "https://claude.ai", icon: "" },
  { name: "Gemini", url: "https://gemini.google.com", icon: "" },
  { name: "Perplexity", url: "https://www.perplexity.ai", icon: "" },
  { name: "Claude Project", url: "https://claude.ai/project/", icon: "" },
  { name: "ChatGPT GPT", url: "https://chatgpt.com/g/", icon: "" },
  { name: "Gemini Gem", url: "https://gemini.google.com/gem/", icon: "" },
  { name: "Perplexity Space", url: "https://www.perplexity.ai/collections/", icon: "" },
  { name: "OpenEvidence", url: "https://www.openevidence.com", icon: "" },
  { name: "Calendar", url: "system://calendar", icon: "📅" },
  { name: "Reminders", url: "system://reminders", icon: "☑️" },
];

/** Monogram tile colours (match theme.css), keyed by a lowercase name fragment. */
const TILE_COLORS: [string, string][] = [
  ["google", "var(--tile-google)"],
  ["chatgpt", "var(--tile-chatgpt)"],
  ["claude", "var(--tile-claude)"],
  ["gemini", "var(--tile-gemini)"],
  ["perplexity", "var(--tile-perplexity)"],
  ["openevidence", "var(--tile-openevidence)"],
];

function tileColor(name: string): string {
  const n = name.toLowerCase();
  return TILE_COLORS.find(([k]) => n.includes(k))?.[1] ?? "var(--tile-other)";
}

let destinations: Destination[] = [];
let editingId: string | null = null; // Currently editing destination ID
let reordering = false;
let appVersion = "";
let shortcuts: ShortcutConfig = {
  toggle_sidebar: "Command+Shift+A",
  screenshot: "Command+Shift+S",
  export: "Command+Shift+E",
};

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const listEl = $("destinations-list");
const addBtn = $("add-btn");
const saveBtn = $("save-btn");
const cancelBtn = $("cancel-btn");
const presetChips = $("preset-chips");
const viewMain = $("view-main");
const viewSite = $("view-site");

interface AppSettings {
  edge_hover_enabled: boolean;
  native_material: boolean;
  background_unload_secs: number;
  auto_send_first: boolean;
  text_from_other_apps: string;
  fixed_destination_id: string | null;
  last_used_destination_id: string | null;
  auto_submit: boolean;
  three_finger_lookup: boolean;
  auto_check_updates: boolean;
  auto_install_updates: boolean;
  language: string;
}

const DEFAULT_SETTINGS: AppSettings = {
  edge_hover_enabled: true,
  native_material: true,
  background_unload_secs: 300,
  auto_send_first: false,
  text_from_other_apps: "choose",
  fixed_destination_id: null,
  last_used_destination_id: null,
  auto_submit: true,
  three_finger_lookup: false,
  auto_check_updates: true,
  auto_install_updates: false,
  language: "zh-TW",
};

let appSettings: AppSettings = { ...DEFAULT_SETTINGS };

/**
 * Persist a partial change. Re-reads the stored settings first so values the
 * backend updates on its own (last_used_destination_id) are never overwritten
 * by a stale copy held by this window.
 */
async function saveSettings(patch: Partial<AppSettings>) {
  let base = appSettings;
  try { base = await invoke<AppSettings>("get_app_settings"); } catch (_e) {}
  appSettings = { ...base, ...patch };
  try { await invoke("save_app_settings", { settings: appSettings }); } catch (_e) {}
}

interface UpdateInfo {
  current: string;
  latest: string;
  available: boolean;
  notes: string;
  page_url: string;
}

interface UpdateStatus {
  state: string;
  message: string;
  info: UpdateInfo | null;
}

function renderUpdate(st: UpdateStatus) {
  const msg = $("update-message");
  const install = $("update-install");
  const check = $<HTMLButtonElement>("update-check");
  const notes = $("update-notes");
  const busy = ["checking", "downloading", "installing"].includes(st.state);
  msg.textContent = st.message || (st.state === "idle" ? t("upd.notChecked") : st.state);
  msg.className = st.state === "error" ? "error" : st.state === "available" ? "available" : "";
  check.disabled = busy;
  const available = !!st.info?.available && !busy;
  install.classList.toggle("hidden", !available);
  if (st.info?.available && st.info.notes) {
    notes.classList.remove("hidden");
    $("update-notes-body").textContent = st.info.notes;
  } else {
    notes.classList.add("hidden");
  }
}

let lastUpdate: UpdateStatus | null = null;

function renderSignature() {
  $("signature").textContent = appVersion ? t("s.signature", { v: appVersion }) : "";
}

async function setupUpdates() {
  const show = (st: UpdateStatus) => { lastUpdate = st; renderUpdate(st); };
  onLangChange(() => { if (lastUpdate) renderUpdate(lastUpdate); renderSignature(); });
  try {
    appVersion = "v" + (await invoke<string>("get_app_version"));
    $("app-version").textContent = appVersion;
    renderSignature();
    show(await invoke<UpdateStatus>("get_update_status"));
  } catch (_e) {}
  listen<UpdateStatus>("update-status", (e) => show(e.payload)).catch(() => {});
  $("update-check").addEventListener("click", async () => {
    try { await invoke("check_for_updates"); } catch (_e) { /* status event shows the error */ }
  });
  $("update-install").addEventListener("click", async () => {
    try {
      await invoke("install_update");
    } catch (e) {
      if (confirm(`${e}\n\n${t("upd.manualPrompt")}`)) invoke("open_release_page").catch(() => {});
    }
  });
  $("open-github").addEventListener("click", () => { invoke("open_release_page").catch(() => {}); });
}

interface Diagnostics {
  activity_work: number;
  loaded_pages: number;
  total_pages: number;
  system_glass: boolean;
  accessibility_trusted: boolean;
  double_copy_active: boolean;
}

// ─── Settings toggles / selects ────────────────────────────

function renderMode() {
  const mode = appSettings.text_from_other_apps;
  document.querySelectorAll<HTMLButtonElement>("#seg-mode button").forEach((b) => {
    b.setAttribute("aria-checked", String(b.dataset.mode === mode));
  });
  $("fixed-row").classList.toggle("hidden", mode !== "fixed");
}

/** Fill the "Send to" select from the destination list and select the stored one. */
function renderFixedSelect() {
  const sel = $<HTMLSelectElement>("opt-fixed-dest");
  const sorted = [...destinations].sort((a, b) => a.order - b.order);
  sel.innerHTML = "";
  sorted.forEach((d) => {
    const o = document.createElement("option");
    o.value = d.id;
    o.textContent = d.name;
    sel.appendChild(o);
  });
  const id = appSettings.fixed_destination_id;
  sel.value = id && sorted.some((d) => d.id === id) ? id : (sorted[0]?.id ?? "");
}

async function setupSettings() {
  const edge = $<HTMLInputElement>("opt-edge-hover");
  const autoSubmit = $<HTMLInputElement>("opt-auto-submit");
  const three = $<HTMLInputElement>("opt-three-finger");
  const autoCheck = $<HTMLInputElement>("opt-auto-check");
  const autoInstall = $<HTMLInputElement>("opt-auto-install");
  const language = $<HTMLSelectElement>("opt-language");
  const fixedSel = $<HTMLSelectElement>("opt-fixed-dest");

  try {
    appSettings = await invoke<AppSettings>("get_app_settings");
  } catch {
    appSettings = { ...DEFAULT_SETTINGS }; // not inside Tauri: still render the layout
  }
  edge.checked = appSettings.edge_hover_enabled;
  autoSubmit.checked = appSettings.auto_submit;
  three.checked = appSettings.three_finger_lookup;
  autoCheck.checked = appSettings.auto_check_updates;
  autoInstall.checked = appSettings.auto_install_updates;
  autoInstall.disabled = !autoCheck.checked;
  language.value = getLang();
  renderMode();
  renderFixedSelect();

  edge.addEventListener("change", () => saveSettings({ edge_hover_enabled: edge.checked }));
  autoSubmit.addEventListener("change", () => saveSettings({ auto_submit: autoSubmit.checked }));
  three.addEventListener("change", () => saveSettings({ three_finger_lookup: three.checked }));
  autoCheck.addEventListener("change", () => {
    autoInstall.disabled = !autoCheck.checked;
    saveSettings({ auto_check_updates: autoCheck.checked });
  });
  autoInstall.addEventListener("change", () => saveSettings({ auto_install_updates: autoInstall.checked }));
  language.addEventListener("change", async () => {
    setLang(normalizeLang(language.value) as Lang);
    await saveSettings({ language: normalizeLang(language.value) });
  });
  // Another window (or a restart) may change the language; keep the picker in sync.
  onLangChange((l) => { language.value = l; });

  document.querySelectorAll<HTMLButtonElement>("#seg-mode button").forEach((b) => {
    b.addEventListener("click", async () => {
      const mode = b.dataset.mode!;
      appSettings = { ...appSettings, text_from_other_apps: mode };
      renderMode();
      const patch: Partial<AppSettings> = { text_from_other_apps: mode };
      if (mode === "fixed") {
        renderFixedSelect();
        if (fixedSel.value) patch.fixed_destination_id = fixedSel.value;
      }
      await saveSettings(patch);
    });
  });
  fixedSel.addEventListener("change", () => saveSettings({ fixed_destination_id: fixedSel.value || null }));

  $("open-trackpad").addEventListener("click", () => { invoke("open_trackpad_settings").catch(() => {}); });

  // Accessibility re-grant row: only shown while ⌘C ⌘C is inactive.
  const axRow = $("copy-ax-row");
  $("copy-ax-open").addEventListener("click", () => { invoke("regrant_accessibility").catch(() => {}); });
  const refresh = async () => {
    try {
      const d = await invoke<Diagnostics>("get_diagnostics");
      axRow.classList.toggle("hidden", d.double_copy_active);
    } catch (_e) {}
  };
  refresh();
  setInterval(() => { if (!document.hidden) refresh(); }, 5000);
}

/** Texts that are built in code rather than marked up with data-i18n. */
function applyDynamicTexts() {
  $("form-title").textContent = t(editingId ? "s.site.editTitle" : "s.site.addTitle");
  saveBtn.textContent = t(editingId ? "common.update" : "common.save");
  $("reorder-btn").textContent = t(reordering ? "s.done" : "s.reorder");
}

// ─── Launch at login ───────────────────────────────────────

async function setupAutostart() {
  const box = $<HTMLInputElement>("opt-autostart");
  const sync = async () => {
    try {
      box.checked = await invoke<boolean>("get_autostart");
      box.disabled = false;
    } catch (_e) {
      box.disabled = true;
    }
  };
  await sync();
  // Re-read the real state whenever the window comes back (System Settings may change it).
  document.addEventListener("visibilitychange", () => { if (!document.hidden) sync(); });
  window.addEventListener("focus", sync);
  box.addEventListener("change", async () => {
    try {
      box.checked = await invoke<boolean>("set_autostart", { enabled: box.checked });
    } catch (e) {
      alert(t("gen.autostartFailed") + e);
      await sync();
    }
  });
}

window.addEventListener("DOMContentLoaded", async () => {
  await initI18n();
  applyDynamicTexts();
  onLangChange(() => {
    applyDynamicTexts();
    renderList();
    renderPresets();
    cancelAllRecordings();
  });
  // Load destinations first: the "Send to" select is built from them.
  await loadDestinations();
  const steps: Array<() => unknown> = [setupAutostart, setupSettings, setupUpdates, loadShortcuts];
  for (const step of steps) {
    try { await step(); } catch (e) { console.warn("settings: step failed", e); }
  }
  renderPresets();
  setupListeners();
  setupShortcutEditing();
});

// ─── Pages ─────────────────────────────────────────────────

function showSite() {
  viewMain.classList.add("hidden");
  viewSite.classList.remove("hidden");
  $("catalog-section").classList.toggle("hidden", !!editingId);
  $("delete-btn").classList.toggle("hidden", !editingId);
  applyDynamicTexts();
  window.scrollTo(0, 0);
}

function showMain() {
  viewSite.classList.add("hidden");
  viewMain.classList.remove("hidden");
  editingId = null;
  applyDynamicTexts();
}

// ─── Destinations ──────────────────────────────────────────

async function loadDestinations() {
  try {
    destinations = await invoke<Destination[]>("get_destinations");
  } catch (_e) {
    destinations = [];
    if ((import.meta as any).env?.DEV && !(window as any).__TAURI_INTERNALS__) {
      // Browser-only preview (no Tauri backend): show sample rows so the layout is visible.
      destinations = [
        { id: "a", name: "Google", url: "https://www.google.com", icon: "", order: 0, clip_prompt: "" },
        { id: "b", name: "ChatGPT", url: "https://chat.openai.com", icon: "", order: 1, clip_prompt: "" },
        { id: "c", name: "Claude", url: "https://claude.ai", icon: "", order: 2, clip_prompt: "請用繁體中文回答：" },
      ];
    }
  }
  renderList();
}

function iconHtml(d: Destination): string {
  if (d.icon && d.icon.trim()) return `<span class="icon">${escapeHtml(d.icon)}</span>`;
  const letter = escapeHtml((d.name.trim()[0] ?? "?").toUpperCase());
  try {
    const host = new URL(d.url).hostname;
    if (host) {
      return `<span class="icon" data-letter="${letter}" data-color="${tileColor(d.name)}"><img src="https://www.google.com/s2/favicons?domain=${encodeURIComponent(host)}&sz=64" width="28" height="28" alt=""></span>`;
    }
  } catch { /* fall through to monogram */ }
  return `<span class="icon" style="background:${tileColor(d.name)};color:#fff;font-weight:600;font-size:14px">${letter}</span>`;
}

// A favicon that fails to load becomes a monogram tile.
listEl.addEventListener("error", (e) => {
  const img = e.target as HTMLElement;
  if (img.tagName !== "IMG") return;
  const holder = img.parentElement!;
  holder.style.background = holder.dataset.color || "var(--tile-other)";
  holder.style.color = "#fff";
  holder.style.fontWeight = "600";
  holder.style.fontSize = "14px";
  holder.textContent = holder.dataset.letter || "?";
}, true);

function renderList() {
  listEl.innerHTML = "";
  $("sites-group").classList.toggle("reordering", reordering);
  const sorted = [...destinations].sort((a, b) => a.order - b.order);
  renderFixedSelect();
  if (sorted.length === 0) {
    listEl.innerHTML = `<div class="dest-empty">${t("dest.empty")}</div>`;
    return;
  }
  sorted.forEach((d, idx) => {
    const el = document.createElement("div");
    el.className = "dest-item";
    const sub = (idx === 0 ? `<span class="default">${t("s.default")}</span> · ` : "") + escapeHtml(d.url);
    el.innerHTML = `
      ${iconHtml(d)}
      <div class="info">
        <div class="name">${escapeHtml(d.name)}</div>
        <div class="sub">${sub}</div>
      </div>
      <div class="reorder-btns">
        <button class="icon-btn up-btn" title="${t("dest.moveUp")}" aria-label="${t("dest.moveUp")}" ${idx === 0 ? "disabled" : ""}>↑</button>
        <button class="icon-btn down-btn" title="${t("dest.moveDown")}" aria-label="${t("dest.moveDown")}" ${idx === sorted.length - 1 ? "disabled" : ""}>↓</button>
        <button class="icon-btn remove-btn" title="${t("dest.remove")}" aria-label="${t("dest.remove")}">✕</button>
      </div>
      <span class="chev" aria-hidden="true">›</span>
    `;
    el.addEventListener("click", () => { if (!reordering) startEdit(d); });
    el.querySelector(".up-btn")!.addEventListener("click", () => moveDestination(d.id, -1));
    el.querySelector(".down-btn")!.addEventListener("click", () => moveDestination(d.id, 1));
    el.querySelector(".remove-btn")!.addEventListener("click", () => removeDestination(d.id));
    listEl.appendChild(el);
  });
}

async function removeDestination(id: string) {
  try { await invoke("remove_destination", { id }); } catch (_e) {}
  destinations = destinations.filter((x) => x.id !== id);
  renderList();
}

async function moveDestination(id: string, direction: number) {
  const sorted = [...destinations].sort((a, b) => a.order - b.order);
  const idx = sorted.findIndex((d) => d.id === id);
  if (idx < 0) return;
  const newIdx = idx + direction;
  if (newIdx < 0 || newIdx >= sorted.length) return;

  // Swap orders
  const temp = sorted[idx].order;
  sorted[idx].order = sorted[newIdx].order;
  sorted[newIdx].order = temp;

  // Build ordered IDs and send to backend
  sorted.sort((a, b) => a.order - b.order);
  const orderedIds = sorted.map((d) => d.id);
  try {
    await invoke("reorder_destinations", { orderedIds });
    destinations = sorted;
    renderList();
  } catch (_e) {}
}

function startEdit(d: Destination) {
  editingId = d.id;
  $<HTMLInputElement>("new-name").value = d.name;
  $<HTMLInputElement>("new-url").value = d.url;
  $<HTMLInputElement>("new-icon").value = d.icon;
  $<HTMLTextAreaElement>("new-clip-prompt").value = d.clip_prompt || "";
  showSite();
}

function renderPresets() {
  presetChips.innerHTML = "";
  PRESETS.forEach((p) => {
    const chip = document.createElement("button");
    chip.className = "preset-chip";
    if (p.icon && p.icon.trim()) {
      chip.textContent = `${p.icon} ${p.name}`;
    } else {
      chip.innerHTML = `<img src="https://www.google.com/s2/favicons?domain=${new URL(p.url).hostname}&sz=32" width="16" height="16" alt="" style="vertical-align:middle"> ${escapeHtml(p.name)}`;
      chip.querySelector("img")!.addEventListener("error", (e) => (e.target as HTMLElement).replaceWith(document.createTextNode("🌐")));
    }
    chip.addEventListener("click", () => {
      $<HTMLInputElement>("new-name").value = p.name;
      $<HTMLInputElement>("new-url").value = p.url;
      $<HTMLInputElement>("new-icon").value = p.icon;
    });
    presetChips.appendChild(chip);
  });

  // "+ Custom" chip: clear the fields for a hand-typed site
  const addChip = document.createElement("button");
  addChip.className = "preset-chip preset-chip-add";
  addChip.textContent = t("dest.custom");
  addChip.addEventListener("click", () => {
    const nameInput = $<HTMLInputElement>("new-name");
    nameInput.value = "";
    $<HTMLInputElement>("new-url").value = "";
    $<HTMLInputElement>("new-icon").value = "";
    applyI18n(viewSite);
    nameInput.focus();
  });
  presetChips.appendChild(addChip);
}

function setupListeners() {
  addBtn.addEventListener("click", () => {
    resetForm();
    showSite();
  });

  $("reorder-btn").addEventListener("click", () => {
    reordering = !reordering;
    applyDynamicTexts();
    renderList();
  });

  const leave = () => { showMain(); resetForm(); };
  cancelBtn.addEventListener("click", leave);
  $("back-btn").addEventListener("click", leave);

  $("delete-btn").addEventListener("click", async () => {
    if (!editingId) return;
    if (!confirm(t("s.site.delete") + "?")) return;
    await removeDestination(editingId);
    leave();
  });

  saveBtn.addEventListener("click", async () => {
    const name = $<HTMLInputElement>("new-name").value.trim();
    const url = $<HTMLInputElement>("new-url").value.trim();
    const icon = $<HTMLInputElement>("new-icon").value.trim();
    const clipPrompt = $<HTMLTextAreaElement>("new-clip-prompt").value;
    if (!name || !url) {
      alert(t("dest.required"));
      return;
    }

    try {
      if (editingId) {
        // Update existing destination
        const updated = await invoke<Destination>("update_destination", {
          id: editingId, name, url, icon, clipPrompt,
        });
        const idx = destinations.findIndex((d) => d.id === editingId);
        if (idx >= 0) destinations[idx] = updated;
      } else {
        // Add new destination
        const newDest = await invoke<Destination>("add_destination", {
          name, url, icon, clipPrompt,
        });
        destinations.push(newDest);
      }
      showMain();
      resetForm();
      renderList();
    } catch (e) {
      alert(t("dest.saveFailed") + e);
    }
  });
}

function resetForm() {
  editingId = null;
  $<HTMLInputElement>("new-name").value = "";
  $<HTMLInputElement>("new-url").value = "";
  $<HTMLInputElement>("new-icon").value = "";
  $<HTMLTextAreaElement>("new-clip-prompt").value = "";
  applyDynamicTexts();
}

// ─── Shortcuts ─────────────────────────────────────────────

async function loadShortcuts() {
  try {
    shortcuts = await invoke<ShortcutConfig>("get_shortcuts");
  } catch (_e) {}
  renderShortcuts();
}

/** Convert internal format "Command+Shift+A" to display format "⌘⇧A" */
function toDisplay(s: string): string {
  return s
    .replace(/Command\+/gi, "⌘")
    .replace(/Shift\+/gi, "⇧")
    .replace(/Alt\+/gi, "⌥")
    .replace(/Control\+/gi, "⌃")
    .replace(/Option\+/gi, "⌥");
}

/** Convert a KeyboardEvent to internal format like "Command+Shift+A" */
function eventToShortcut(e: KeyboardEvent): string | null {
  // Need at least one modifier
  if (!e.metaKey && !e.ctrlKey && !e.altKey) return null;

  const parts: string[] = [];
  if (e.metaKey) parts.push("Command");
  if (e.ctrlKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");

  // Get the key — ignore lone modifier presses
  const key = e.key;
  if (["Meta", "Control", "Alt", "Shift", "CapsLock"].includes(key)) return null;

  // Map key to code name
  let keyName: string;
  if (key.length === 1 && /[a-zA-Z0-9]/.test(key)) {
    keyName = key.toUpperCase();
  } else if (key.startsWith("F") && /^F\d{1,2}$/.test(key)) {
    keyName = key;
  } else {
    switch (key) {
      case " ": keyName = "Space"; break;
      case "Enter": keyName = "Enter"; break;
      case "Tab": keyName = "Tab"; break;
      case "Escape": keyName = "Escape"; break;
      case "Backspace": keyName = "Backspace"; break;
      case "Delete": keyName = "Delete"; break;
      case "ArrowUp": keyName = "Up"; break;
      case "ArrowDown": keyName = "Down"; break;
      case "ArrowLeft": keyName = "Left"; break;
      case "ArrowRight": keyName = "Right"; break;
      default: return null;
    }
  }

  parts.push(keyName);
  return parts.join("+");
}

function renderShortcuts() {
  const actions: Record<string, string> = {
    toggle_sidebar: shortcuts.toggle_sidebar,
    screenshot: shortcuts.screenshot,
    export: shortcuts.export,
  };

  for (const [action, value] of Object.entries(actions)) {
    const row = document.querySelector(`.shortcut-row[data-action="${action}"]`);
    if (!row) continue;
    const kbd = row.querySelector(".shortcut-key") as HTMLElement;
    if (kbd) {
      kbd.textContent = toDisplay(value);
    }
  }
}

let activeRecording: string | null = null;

function setupShortcutEditing() {
  const editableKbds = document.querySelectorAll<HTMLElement>(".shortcut-key");

  editableKbds.forEach((kbd) => {
    const row = kbd.closest(".shortcut-row") as HTMLElement;
    const action = row?.dataset.action;
    if (!action) return;

    kbd.addEventListener("click", () => {
      // If already recording this one, cancel
      if (activeRecording === action) {
        cancelRecording(kbd, action);
        return;
      }
      // Cancel any other active recording
      cancelAllRecordings();
      // Start recording
      activeRecording = action;
      kbd.classList.add("recording");
      kbd.textContent = t("keys.recording");
    });
  });

  // Global keydown listener for recording
  document.addEventListener("keydown", async (e) => {
    if (!activeRecording) return;

    e.preventDefault();
    e.stopPropagation();

    // Escape cancels
    if (e.key === "Escape") {
      cancelAllRecordings();
      return;
    }

    const shortcutStr = eventToShortcut(e);
    if (!shortcutStr) return; // Just a modifier key press, keep waiting

    const action = activeRecording;
    activeRecording = null;

    // Update local state
    (shortcuts as any)[action] = shortcutStr;

    // Update display
    renderShortcuts();
    cancelAllRecordings();

    // Save to backend
    try {
      await invoke("save_shortcuts", { config: shortcuts });
    } catch (err) {
      alert(t("keys.saveFailed") + err);
      // Reload from backend
      await loadShortcuts();
    }
  });

  // Click outside cancels recording
  document.addEventListener("click", (e) => {
    if (!activeRecording) return;
    const target = e.target as HTMLElement;
    if (!target.classList.contains("shortcut-key")) {
      cancelAllRecordings();
    }
  });
}

function cancelRecording(kbd: HTMLElement, action: string) {
  activeRecording = null;
  kbd.classList.remove("recording");
  kbd.textContent = toDisplay((shortcuts as any)[action]);
}

function cancelAllRecordings() {
  activeRecording = null;
  document.querySelectorAll<HTMLElement>(".shortcut-key.recording").forEach((el) => {
    el.classList.remove("recording");
  });
  renderShortcuts();
}
