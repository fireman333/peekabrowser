import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { applyMaterial, destIcon, escapeHtml, hydrateIcons, installFaviconFallback } from "./icons";

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
let current: Destination[] = [];
let busy = false;

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
  applyMaterial(invoke);
  installFaviconFallback(document.getElementById("picker-list")!);

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) refreshPicker();
  });
  listen("show-picker", () => refreshPicker()).catch(console.error);

  document.getElementById("picker-close")!.addEventListener("click", dismissPicker);

  // Keyboard works only when the panel has key focus (it is non-activating so
  // the source app keeps focus); the mouse remains the primary path.
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      dismissPicker();
    } else if (/^[1-9]$/.test(e.key)) {
      const dest = current[Number(e.key) - 1];
      if (dest) pick(dest);
    }
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
    title.textContent = "Send screenshot to…";
    preview.innerHTML = `<img src="${data.image_preview}" alt="Screenshot preview">`;
  } else if (data.kind === "text" && data.text) {
    title.textContent = "Send text to…";
    const snippet = data.text.length > 140 ? data.text.slice(0, 140) + "…" : data.text;
    preview.innerHTML = `<p>${escapeHtml(snippet)}</p>`;
  } else {
    title.textContent = "Send to…";
    preview.innerHTML = "";
  }
}

function renderPicker(data: PickerData) {
  renderPreview(data);
  const list = document.getElementById("picker-list")!;
  list.innerHTML = "";
  current = data.destinations;

  data.destinations.forEach((dest, idx) => {
    const btn = document.createElement("button");
    btn.className = "picker-btn";
    btn.setAttribute("role", "option");
    const prefix = dest.clip_prompt?.trim();
    btn.title = prefix ? `${dest.name} — prompt: ${prefix}` : dest.name;
    btn.innerHTML = `
      <span class="picker-icon">${destIcon(dest, 22)}</span>
      <span class="picker-name">${escapeHtml(dest.name)}</span>
      ${idx < 9 ? `<kbd class="picker-key" aria-hidden="true">${idx + 1}</kbd>` : ""}
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
