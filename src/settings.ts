import { invoke } from "@tauri-apps/api/core";

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

window.addEventListener("DOMContentLoaded", async () => {
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
  sorted.forEach((d, idx) => {
    const el = document.createElement("div");
    el.className = "dest-item";
    const iconHtml = d.icon && d.icon.trim()
      ? d.icon
      : (() => { try { return `<img src="https://www.google.com/s2/favicons?domain=${new URL(d.url).hostname}&sz=32" width="20" height="20" style="vertical-align:middle" onerror="this.replaceWith(document.createTextNode('🌐'))">`; } catch { return '🌐'; } })();
    el.innerHTML = `
      <span class="icon">${iconHtml}</span>
      <div class="info">
        <div class="name">${d.name}</div>
        <div class="url">${d.url}</div>
      </div>
      <div class="reorder-btns">
        <button class="reorder-btn up-btn" title="Move up" ${idx === 0 ? "disabled" : ""}>↑</button>
        <button class="reorder-btn down-btn" title="Move down" ${idx === sorted.length - 1 ? "disabled" : ""}>↓</button>
      </div>
      <button class="edit-btn" title="Edit">✎</button>
      <button class="remove-btn" title="Remove">✕</button>
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
  // Update form title and button
  const formTitle = addForm.querySelector("h2");
  if (formTitle) formTitle.textContent = "Edit Destination";
  saveBtn.textContent = "Update";
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
  addChip.textContent = "+ Custom";
  addChip.addEventListener("click", () => {
    const nameInput = document.getElementById("new-name") as HTMLInputElement;
    const urlInput = document.getElementById("new-url") as HTMLInputElement;
    const iconInput = document.getElementById("new-icon") as HTMLInputElement;
    nameInput.value = "";
    urlInput.value = "";
    iconInput.value = "";
    nameInput.placeholder = "Enter name";
    urlInput.placeholder = "Paste URL";
    iconInput.placeholder = "Leave empty to use website icon";
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
      alert("Name and URL are required");
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
      alert("Failed to save destination: " + e);
    }
  });
}

function resetForm() {
  editingId = null;
  (document.getElementById("new-name") as HTMLInputElement).value = "";
  (document.getElementById("new-url") as HTMLInputElement).value = "";
  (document.getElementById("new-icon") as HTMLInputElement).value = "";
  (document.getElementById("new-clip-prompt") as HTMLTextAreaElement).value = "";
  const formTitle = addForm.querySelector("h2");
  if (formTitle) formTitle.textContent = "Add Destination";
  saveBtn.textContent = "Save";
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
      kbd.textContent = "Press keys...";
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
      alert("Failed to save shortcut: " + err);
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
