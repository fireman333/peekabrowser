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

let destinations: Destination[] = [];
let editingId: string | null = null; // Currently editing destination ID
let shortcuts: ShortcutConfig = {
  toggle_sidebar: "Command+Shift+A",
  screenshot: "Command+Shift+S",
  export: "Command+Shift+E",
};

const listEl = document.getElementById("destinations-list")!;
const addBtn = document.getElementById("add-btn")!;
const addForm = document.getElementById("add-form")!;
const saveBtn = document.getElementById("save-btn")!;
const cancelBtn = document.getElementById("cancel-btn")!;
const presetChips = document.getElementById("preset-chips")!;

interface AppSettings {
  edge_hover_enabled: boolean;
  native_material: boolean;
  background_unload_secs: number;
  auto_send_first: boolean;
  auto_check_updates: boolean;
  auto_install_updates: boolean;
  language: string;
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
  const msg = document.getElementById("update-message")!;
  const install = document.getElementById("update-install")!;
  const check = document.getElementById("update-check") as HTMLButtonElement;
  const notes = document.getElementById("update-notes")!;
  const busy = ["checking", "downloading", "installing"].includes(st.state);
  msg.textContent = st.message || (st.state === "idle" ? t("upd.notChecked") : st.state);
  msg.className = st.state === "error" ? "error" : st.state === "available" ? "available" : "";
  check.disabled = busy;
  const available = !!st.info?.available && !busy;
  install.classList.toggle("hidden", !available);
  if (st.info?.available && st.info.notes) {
    notes.classList.remove("hidden");
    document.getElementById("update-notes-body")!.textContent = st.info.notes;
  } else {
    notes.classList.add("hidden");
  }
}

let lastUpdate: UpdateStatus | null = null;

async function setupUpdates() {
  const show = (st: UpdateStatus) => { lastUpdate = st; renderUpdate(st); };
  onLangChange(() => { if (lastUpdate) renderUpdate(lastUpdate); });
  try {
    document.getElementById("app-version")!.textContent = "v" + (await invoke<string>("get_app_version"));
    show(await invoke<UpdateStatus>("get_update_status"));
  } catch (_e) {}
  listen<UpdateStatus>("update-status", (e) => show(e.payload)).catch(() => {});
  document.getElementById("update-check")!.addEventListener("click", async () => {
    try { await invoke("check_for_updates"); } catch (_e) { /* status event shows the error */ }
  });
  document.getElementById("update-install")!.addEventListener("click", async () => {
    try {
      await invoke("install_update");
    } catch (e) {
      if (confirm(`${e}\n\n${t("upd.manualPrompt")}`)) invoke("open_release_page").catch(() => {});
    }
  });
}

interface Diagnostics {
  activity_work: number;
  loaded_pages: number;
  total_pages: number;
  system_glass: boolean;
  accessibility_trusted: boolean;
}

async function setupPowerSettings() {
  const edge = document.getElementById("opt-edge-hover") as HTMLInputElement;
  const material = document.getElementById("opt-material") as HTMLInputElement;
  const unload = document.getElementById("opt-unload") as HTMLSelectElement;
  const autoFirst = document.getElementById("opt-auto-first") as HTMLInputElement;
  const autoCheck = document.getElementById("opt-auto-check") as HTMLInputElement;
  const autoInstall = document.getElementById("opt-auto-install") as HTMLInputElement;
  const language = document.getElementById("opt-language") as HTMLSelectElement;
  let current: AppSettings;
  try {
    current = await invoke<AppSettings>("get_app_settings");
  } catch {
    return;
  }
  edge.checked = current.edge_hover_enabled;
  material.checked = current.native_material;
  autoFirst.checked = current.auto_send_first;
  autoCheck.checked = current.auto_check_updates;
  autoInstall.checked = current.auto_install_updates;
  autoInstall.disabled = !autoCheck.checked;
  language.value = getLang();
  unload.value = String(current.background_unload_secs);
  if (!unload.value) unload.value = "300";
  const save = async () => {
    current = {
      ...current,
      edge_hover_enabled: edge.checked,
      native_material: material.checked,
      background_unload_secs: Number(unload.value) || 300,
      auto_send_first: autoFirst.checked,
      auto_check_updates: autoCheck.checked,
      auto_install_updates: autoInstall.checked,
      language: normalizeLang(language.value),
    };
    autoInstall.disabled = !autoCheck.checked;
    try { await invoke("save_app_settings", { settings: current }); } catch (_e) {}
  };
  edge.addEventListener("change", save);
  material.addEventListener("change", save);
  unload.addEventListener("change", save);
  autoFirst.addEventListener("change", save);
  autoCheck.addEventListener("change", save);
  autoInstall.addEventListener("change", save);
  language.addEventListener("change", async () => {
    setLang(normalizeLang(language.value) as Lang);
    await save();
  });
  // Another window (or a restart) may change the language; keep the picker in sync.
  onLangChange((l) => { language.value = l; });

  const diag = document.getElementById("diagnostics")!;
  let last: Diagnostics | null = null;
  const render = () => {
    if (!last) return;
    const d = last;
    diag.textContent = [
      t("diag.status", { loaded: d.loaded_pages, total: d.total_pages }),
      d.activity_work ? t("diag.activity", { n: d.activity_work }) : t("diag.noActivity"),
      t("diag.material", { m: d.system_glass ? "Liquid Glass" : t("diag.vibrancy") }),
      d.accessibility_trusted ? t("diag.keyTriggered") : t("diag.sampling"),
    ].join(" · ");
  };
  const refresh = async () => {
    try {
      last = await invoke<Diagnostics>("get_diagnostics");
      render();
    } catch (_e) {}
  };
  onLangChange(render);
  refresh();
  setInterval(() => { if (!document.hidden) refresh(); }, 5000);
}

/** Texts that are built in code rather than marked up with data-i18n. */
function applyDynamicTexts() {
  document.querySelectorAll<HTMLOptionElement>("#opt-unload option[data-min]").forEach((o) => {
    o.textContent = t("power.min", { n: o.dataset.min! });
  });
  const formTitle = document.getElementById("form-title");
  if (formTitle) formTitle.textContent = t(editingId ? "dest.editTitle" : "dest.addTitle");
  saveBtn.textContent = t(editingId ? "common.update" : "common.save");
}

// ─── Launch at login ───────────────────────────────────────

async function setupAutostart() {
  const box = document.getElementById("opt-autostart") as HTMLInputElement;
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
  setupAutostart();
  setupPowerSettings();
  setupUpdates();
  await loadDestinations();
  await loadShortcuts();
  renderPresets();
  setupListeners();
  setupShortcutEditing();
});

// ─── Destinations ──────────────────────────────────────────

async function loadDestinations() {
  try {
    destinations = await invoke<Destination[]>("get_destinations");
  } catch (_e) {
    destinations = [];
  }
  renderList();
}

function renderList() {
  listEl.innerHTML = "";
  const sorted = [...destinations].sort((a, b) => a.order - b.order);
  if (sorted.length === 0) {
    listEl.innerHTML = `<div class="dest-empty">${t("dest.empty")}</div>`;
    return;
  }
  sorted.forEach((d, idx) => {
    const el = document.createElement("div");
    el.className = "dest-item";
    const iconHtml = d.icon && d.icon.trim()
      ? d.icon
      : (() => { try { return `<img src="https://www.google.com/s2/favicons?domain=${new URL(d.url).hostname}&sz=32" width="20" height="20" style="vertical-align:middle" onerror="this.replaceWith(document.createTextNode('🌐'))">`; } catch { return '🌐'; } })();
    el.innerHTML = `
      <span class="icon">${iconHtml}</span>
      <div class="info">
        <div class="name">${escapeHtml(d.name)}</div>
        <div class="url">${escapeHtml(d.url)}</div>
      </div>
      <div class="reorder-btns">
        <button class="reorder-btn up-btn" title="${t("dest.moveUp")}" aria-label="${t("dest.moveUp")}" ${idx === 0 ? "disabled" : ""}>↑</button>
        <button class="reorder-btn down-btn" title="${t("dest.moveDown")}" aria-label="${t("dest.moveDown")}" ${idx === sorted.length - 1 ? "disabled" : ""}>↓</button>
      </div>
      <button class="edit-btn" title="${t("dest.edit")}" aria-label="${t("dest.edit")}">✎</button>
      <button class="remove-btn" title="${t("dest.remove")}" aria-label="${t("dest.remove")}">✕</button>
    `;
    el.querySelector(".up-btn")!.addEventListener("click", () => moveDestination(d.id, -1));
    el.querySelector(".down-btn")!.addEventListener("click", () => moveDestination(d.id, 1));
    el.querySelector(".edit-btn")!.addEventListener("click", () => {
      startEdit(d);
    });
    el.querySelector(".remove-btn")!.addEventListener("click", async () => {
      try { await invoke("remove_destination", { id: d.id }); } catch (_e) {}
      destinations = destinations.filter((x) => x.id !== d.id);
      renderList();
    });
    listEl.appendChild(el);
  });
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
  (document.getElementById("new-name") as HTMLInputElement).value = d.name;
  (document.getElementById("new-url") as HTMLInputElement).value = d.url;
  (document.getElementById("new-icon") as HTMLInputElement).value = d.icon;
  (document.getElementById("new-clip-prompt") as HTMLTextAreaElement).value = d.clip_prompt || "";
  addForm.classList.remove("hidden");
  applyDynamicTexts();
}

function renderPresets() {
  presetChips.innerHTML = "";
  PRESETS.forEach((p) => {
    const chip = document.createElement("button");
    chip.className = "preset-chip";
    if (p.icon && p.icon.trim()) {
      chip.textContent = `${p.icon} ${p.name}`;
    } else {
      chip.innerHTML = `<img src="https://www.google.com/s2/favicons?domain=${new URL(p.url).hostname}&sz=32" width="16" height="16" style="vertical-align:middle" onerror="this.replaceWith(document.createTextNode('🌐'))"> ${p.name}`;
    }
    chip.addEventListener("click", () => {
      (document.getElementById("new-name") as HTMLInputElement).value = p.name;
      (document.getElementById("new-url") as HTMLInputElement).value = p.url;
      (document.getElementById("new-icon") as HTMLInputElement).value = p.icon;
    });
    presetChips.appendChild(chip);
  });

  // "+ Add" chip for custom destinations
  const addChip = document.createElement("button");
  addChip.className = "preset-chip preset-chip-add";
  addChip.textContent = t("dest.custom");
  addChip.addEventListener("click", () => {
    const nameInput = document.getElementById("new-name") as HTMLInputElement;
    const urlInput = document.getElementById("new-url") as HTMLInputElement;
    const iconInput = document.getElementById("new-icon") as HTMLInputElement;
    nameInput.value = "";
    urlInput.value = "";
    iconInput.value = "";
    applyI18n(addForm);
    nameInput.focus();
  });
  presetChips.appendChild(addChip);
}

function setupListeners() {
  addBtn.addEventListener("click", () => {
    resetForm();
    addForm.classList.remove("hidden");
  });

  cancelBtn.addEventListener("click", () => {
    addForm.classList.add("hidden");
    resetForm();
  });


  saveBtn.addEventListener("click", async () => {
    const name = (document.getElementById("new-name") as HTMLInputElement).value.trim();
    const url = (document.getElementById("new-url") as HTMLInputElement).value.trim();
    const icon = (document.getElementById("new-icon") as HTMLInputElement).value.trim();
    const clipPrompt = (document.getElementById("new-clip-prompt") as HTMLTextAreaElement).value;
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
      addForm.classList.add("hidden");
      resetForm();
      renderList();
    } catch (e) {
      alert(t("dest.saveFailed") + e);
    }
  });
}

function resetForm() {
  editingId = null;
  (document.getElementById("new-name") as HTMLInputElement).value = "";
  (document.getElementById("new-url") as HTMLInputElement).value = "";
  (document.getElementById("new-icon") as HTMLInputElement).value = "";
  (document.getElementById("new-clip-prompt") as HTMLTextAreaElement).value = "";
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
