import { invoke } from "@tauri-apps/api/core";
import { isImeKey } from "./ime";
import { listen } from "@tauri-apps/api/event";
import { applyMaterial, destIcon, escapeHtml, hydrateIcons, icon, installFaviconFallback } from "./icons";
import { initI18n, onLangChange, t } from "./i18n";

interface Destination {
  id: string;
  name: string;
  url: string;
  icon: string;
  clip_prompt: string;
}

interface Lens {
  id: string;
  title: string;
  prompt: string;
}

interface PickerData {
  destinations: Destination[];
  kind: "text" | "image" | "";
  text: string;
  image_preview: string | null;
  /** Destination ID → its prompts (missing for Calendar / Reminders). */
  prompts?: Record<string, Lens[]>;
}

/** A selectable row: a destination, or one of its expanded prompts. */
type Row = { dest: Destination; lens: Lens | null; el: HTMLElement };

let autoDismissTimer: ReturnType<typeof setTimeout> | null = null;
let cursorVisited = false;
// Must match PICK_KEYS in src-tauri/src/picker_keys.rs.
const PICK_KEYS = ["C", "V", "B", "N", "M"];
/** Press-and-hold time that expands a destination's prompts (iOS: 0.4 s). */
const HOLD_MS = 400;
let busy = false;
let lastData: PickerData | null = null;
/** Destination whose prompts are shown inline (one at a time). */
let expandedId: string | null = null;
/** Keyboard selection in `rows` (-1 = none: Return sends the default). */
let selected = -1;
let rows: Row[] = [];
let holdTimer: ReturnType<typeof setTimeout> | null = null;
/** The press that expanded prompts must not also send on its release. */
let suppressClick = false;

const promptsOf = (d: Destination): Lens[] => lastData?.prompts?.[d.id] ?? [];

/** Plain-browser layout preview (`npx vite`): no Tauri backend. */
const isPreview = () => !(window as any).__TAURI_INTERNALS__;

async function refreshPicker() {
  if (isPreview()) {
    expandedId = null;
    selected = -1;
    renderPicker(previewData());
    return;
  }
  try {
    const data = await invoke<PickerData>("get_picker_data");
    if (data.destinations.length > 0) {
      cursorVisited = false;
      busy = false;
      expandedId = null;
      selected = -1;
      renderPicker(data);
      // Long fallback timeout in case the cursor never visits the picker
      scheduleAutoDismiss(15000);
    }
  } catch (e) {
    console.error("get_picker_data failed:", e);
  }
}

/** Browser-only preview (no Tauri backend): sample rows so the layout is visible. */
function previewData(): PickerData {
  const d = (id: string, name: string, url: string): Destination => ({ id, name, url, icon: "", clip_prompt: "" });
  const lens = (id: string, title: string): Lens => ({ id, title, prompt: `${title}: ` });
  return {
    destinations: [d("a", "Google", "https://www.google.com"), d("b", "ChatGPT", "https://chatgpt.com"), d("c", "Claude", "https://claude.ai"), d("d", "Calendar", "system://calendar")],
    kind: "text",
    text: "aspirin renal dosing",
    image_preview: null,
    prompts: {
      a: [lens("general.explain", "Explain simply"), lens("general.summarize", "Summarize"), lens("general.proscons", "Pros and cons"), lens("general.translate", "Translate to English")],
      b: [lens("general.explain", "Explain simply"), lens("x", "My prompt")],
      c: [],
    },
  };
}

window.addEventListener("DOMContentLoaded", () => {
  hydrateIcons();
  initI18n();
  onLangChange(() => { if (lastData) renderPicker(lastData); else renderHint(null); });
  renderHint(null);
  if (!isPreview()) applyMaterial(invoke);
  installFaviconFallback(document.getElementById("picker-list")!);

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) refreshPicker();
  });
  if (isPreview()) refreshPicker();
  else listen("show-picker", () => refreshPicker()).catch(console.error);

  document.getElementById("picker-close")!.addEventListener("click", dismissPicker);

  // C/V/B/N/M and Esc are handled natively (hardware key codes, so they work
  // with any input method, e.g. 注音); Esc here is only a fallback.
  // ↑↓ move through destinations and expanded prompts, → expands (again:
  // collapses), ← collapses, Return sends the selected row (none: the default).
  document.addEventListener("keydown", (e) => {
    if (isImeKey(e)) return;
    if (e.key === "Escape") {
      dismissPicker();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (rows.length === 0) return;
      const step = e.key === "ArrowDown" ? 1 : -1;
      selected = selected < 0 ? (step > 0 ? 0 : rows.length - 1) : Math.min(rows.length - 1, Math.max(0, selected + step));
      markSelected();
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      const row = rows[selected] ?? rows[0];
      if (row && !row.lens) toggleExpand(row.dest, true);
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      const row = rows[selected];
      if (expandedId) {
        const dest = row?.dest.id === expandedId ? row.dest : lastData?.destinations.find((d) => d.id === expandedId);
        if (dest) toggleExpand(dest, true, true);
      }
    } else if (e.key === "Enter") {
      e.preventDefault();
      const row = rows[selected];
      if (row) pick(row.dest, row.lens);
      else if (lastData?.destinations[0]) pick(lastData.destinations[0], null);
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

function renderHint(data: PickerData | null) {
  const hint = document.getElementById("picker-hint")!;
  const anyPrompts = !!data?.destinations.some((d) => (data.prompts?.[d.id] ?? []).length > 0);
  hint.innerHTML = escapeHtml(t("picker.hint")) + (anyPrompts ? `<br>${escapeHtml(t("picker.promptHint"))}` : "");
}

function renderPicker(data: PickerData) {
  lastData = data;
  renderPreview(data);
  renderHint(data);
  const list = document.getElementById("picker-list")!;
  list.innerHTML = "";
  rows = [];

  data.destinations.forEach((dest, idx) => {
    const prompts = promptsOf(dest);
    const expanded = expandedId === dest.id && prompts.length > 0;
    const btn = document.createElement("button");
    btn.className = "picker-btn";
    btn.id = `dest-${idx}`;
    btn.setAttribute("role", "option");
    const prefix = dest.clip_prompt?.trim();
    const key = idx < PICK_KEYS.length ? ` (${PICK_KEYS[idx]})` : "";
    btn.title = (prefix ? `${dest.name} — ${t("picker.prompt")}: ${prefix}` : dest.name) + key;
    if (key) btn.setAttribute("aria-keyshortcuts", PICK_KEYS[idx]);
    if (prompts.length > 0) btn.setAttribute("aria-expanded", String(expanded));
    btn.innerHTML = `
      <span class="picker-icon">${destIcon(dest, 22)}</span>
      <span class="picker-name">${escapeHtml(dest.name)}</span>
      ${idx === 0 ? `<span class="picker-default">${t("picker.defaultTag")} ⏎</span>` : ""}
      ${prompts.length > 0 ? `<span class="picker-badge" title="${escapeHtml(t("picker.promptCount", { n: prompts.length }))}">${icon("sf-chevron-right", 11)}</span>` : ""}
      ${idx < PICK_KEYS.length ? `<kbd class="picker-key" aria-hidden="true">${PICK_KEYS[idx]}</kbd>` : ""}
    `;
    installHold(btn, dest);
    btn.addEventListener("click", () => {
      if (suppressClick) { suppressClick = false; return; }
      pick(dest, null);
    });
    list.appendChild(btn);
    rows.push({ dest, lens: null, el: btn });

    if (expanded) {
      const group = document.createElement("div");
      group.className = "picker-prompts";
      group.setAttribute("role", "group");
      group.setAttribute("aria-label", dest.name);
      prompts.forEach((lens, j) => {
        const p = document.createElement("button");
        p.className = "picker-prompt";
        p.id = `prompt-${idx}-${j}`;
        p.setAttribute("role", "option");
        p.title = lens.prompt.trim();
        p.innerHTML = `<span class="picker-prompt-icon">${icon("sf-text-quote", 13)}</span><span class="picker-prompt-title">${escapeHtml(lens.title)}</span>`;
        p.addEventListener("click", () => pick(dest, lens));
        group.appendChild(p);
        rows.push({ dest, lens, el: p });
      });
      list.appendChild(group);
    }
  });
  markSelected();
  reportSize();
}

/** Press and hold (0.4 s) or right-click a destination: show its prompts. */
function installHold(btn: HTMLElement, dest: Destination) {
  const cancel = () => { if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; } };
  btn.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    cancel();
    suppressClick = false;
    if (promptsOf(dest).length === 0) return;
    holdTimer = setTimeout(() => {
      holdTimer = null;
      suppressClick = true;
      toggleExpand(dest, false);
    }, HOLD_MS);
  });
  btn.addEventListener("pointerup", cancel);
  btn.addEventListener("pointerleave", cancel);
  btn.addEventListener("pointercancel", cancel);
  btn.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    cancel();
    toggleExpand(dest, false);
  });
}

/** Expand a destination's prompts, or collapse them when already shown. */
function toggleExpand(dest: Destination, fromKeyboard: boolean, collapseOnly = false) {
  if (!lastData || promptsOf(dest).length === 0) return;
  const expanding = expandedId !== dest.id && !collapseOnly;
  expandedId = expanding ? dest.id : null;
  clearAutoDismiss();
  renderPicker(lastData);
  // Keep the selection on the destination row (first prompt when expanding by keyboard).
  const destRow = rows.findIndex((r) => r.dest.id === dest.id && !r.lens);
  if (fromKeyboard) selected = expanding ? destRow + 1 : destRow;
  else if (selected >= 0) selected = destRow;
  markSelected();
}

function markSelected() {
  const list = document.getElementById("picker-list")!;
  rows.forEach((r, i) => r.el.classList.toggle("kb-selected", i === selected));
  const row = rows[selected];
  if (row) {
    list.setAttribute("aria-activedescendant", row.el.id);
    row.el.scrollIntoView({ block: "nearest" });
  } else {
    list.removeAttribute("aria-activedescendant");
  }
}

/** Ask the window to grow by the expanded prompts' height (0 when collapsed). */
function reportSize() {
  requestAnimationFrame(() => {
    const group = document.querySelector<HTMLElement>(".picker-prompts");
    const gap = parseFloat(getComputedStyle(document.getElementById("picker-list")!).rowGap) || 0;
    const extra = group ? Math.ceil(group.getBoundingClientRect().height + gap) : 0;
    if (!isPreview()) invoke("resize_picker", { extra }).catch(() => {});
  });
}

async function pick(dest: Destination, lens: Lens | null) {
  if (busy) return;
  busy = true;
  clearAutoDismiss();
  try {
    await invoke("pick_destination", { id: dest.id, promptId: lens?.id ?? null });
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
