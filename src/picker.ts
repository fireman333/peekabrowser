import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { applyMaterial, destIcon, escapeHtml, hydrateIcons, installFaviconFallback } from "./icons";
import { initI18n, onLangChange, t } from "./i18n";

interface Destination {
  id: string;
  name: string;
  url: string;
  icon: string;
  clip_prompt: string;
}

interface PickerData {
  destinations: Destination[];
  kind: "text" | "image" | "";
  text: string;
  image_preview: string | null;
}

let autoDismissTimer: ReturnType<typeof setTimeout> | null = null;
let cursorVisited = false;
// Must match PICK_KEYS in src-tauri/src/picker_keys.rs.
const PICK_KEYS = ["C", "V", "B", "N", "M"];
let busy = false;
let lastData: PickerData | null = null;

async function refreshPicker() {
  try {
    const data = await invoke<PickerData>("get_picker_data");
    if (data.destinations.length > 0) {
      cursorVisited = false;
      busy = false;
      renderPicker(data);
      // Long fallback timeout in case the cursor never visits the picker
      scheduleAutoDismiss(15000);
    }
  } catch (e) {
    console.error("get_picker_data failed:", e);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  hydrateIcons();
  initI18n();
  onLangChange(() => { if (lastData) renderPicker(lastData); });
  applyMaterial(invoke);
  installFaviconFallback(document.getElementById("picker-list")!);

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) refreshPicker();
  });
  listen("show-picker", () => refreshPicker()).catch(console.error);

  document.getElementById("picker-close")!.addEventListener("click", dismissPicker);

  // C/V/B/N/M and Esc are handled natively (hardware key codes, so they work
  // with any input method, e.g. 注音); this is only a fallback for Esc.
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") dismissPicker();
  });

  const card = document.getElementById("picker-card")!;
  card.addEventListener("mouseenter", () => {
    cursorVisited = true;
    clearAutoDismiss();
  });
  card.addEventListener("mouseleave", () => {
    if (cursorVisited && !busy) dismissPicker();
  });
});

function renderPreview(data: PickerData) {
  const preview = document.getElementById("picker-preview")!;
  const title = document.getElementById("picker-title")!;
  if (data.kind === "image" && data.image_preview) {
    title.textContent = t("picker.sendShot");
    preview.innerHTML = `<img src="${data.image_preview}" alt="${t("picker.shotAlt")}">`;
  } else if (data.kind === "text" && data.text) {
    title.textContent = t("picker.sendText");
    const snippet = data.text.length > 140 ? data.text.slice(0, 140) + "…" : data.text;
    preview.innerHTML = `<p>${escapeHtml(snippet)}</p>`;
  } else {
    title.textContent = t("picker.sendTo");
    preview.innerHTML = "";
  }
}

function renderPicker(data: PickerData) {
  lastData = data;
  renderPreview(data);
  const list = document.getElementById("picker-list")!;
  list.innerHTML = "";

  data.destinations.forEach((dest, idx) => {
    const btn = document.createElement("button");
    btn.className = "picker-btn";
    btn.setAttribute("role", "option");
    const prefix = dest.clip_prompt?.trim();
    const key = idx < PICK_KEYS.length ? ` (${PICK_KEYS[idx]})` : "";
    btn.title = (prefix ? `${dest.name} — ${t("picker.prompt")}: ${prefix}` : dest.name) + key;
    if (key) btn.setAttribute("aria-keyshortcuts", PICK_KEYS[idx]);
    btn.innerHTML = `
      <span class="picker-icon">${destIcon(dest, 22)}</span>
      <span class="picker-name">${escapeHtml(dest.name)}</span>
      ${idx < PICK_KEYS.length ? `<kbd class="picker-key" aria-hidden="true">${PICK_KEYS[idx]}</kbd>` : ""}
    `;
    btn.addEventListener("click", () => pick(dest));
    list.appendChild(btn);
  });
}

async function pick(dest: Destination) {
  if (busy) return;
  busy = true;
  clearAutoDismiss();
  try {
    await invoke("pick_destination", { id: dest.id });
  } catch (e) {
    console.error("pick_destination failed:", e);
    busy = false;
  }
}

function scheduleAutoDismiss(ms = 3000) {
  clearAutoDismiss();
  autoDismissTimer = setTimeout(dismissPicker, ms);
}

function clearAutoDismiss() {
  if (autoDismissTimer !== null) {
    clearTimeout(autoDismissTimer);
    autoDismissTimer = null;
  }
}

async function dismissPicker() {
  clearAutoDismiss();
  try {
    await invoke("hide_picker_panel");
  } catch (_e) {}
}
