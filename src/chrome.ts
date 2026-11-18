// Top chrome window, above the active page: address capsule, nav cluster and
// (when the destination has 2+ tabs) the tab row. Rust sizes the window and
// shows/hides it together with the page (panel::sync_chrome).
import { invoke } from "@tauri-apps/api/core";
import { isImeKey, trackComposition } from "./ime";
import { listen } from "@tauri-apps/api/event";
import { applyMaterial, destTile, hydrateIcons, icon, installFaviconFallback } from "./icons";
import { initI18n, onLangChange, t } from "./i18n";
import { installTabShortcuts } from "./tab-shortcuts";

interface Destination {
  id: string;
  name: string;
  url: string;
  icon: string;
  order: number;
}

interface PageInfo {
  id: string;
  dest_id: string;
  dest_name: string;
  dest_icon: string;
  label: string | null;
  state: "active" | "background" | "unloaded";
  generating: boolean;
  unseen: boolean;
  title: string;
  url: string;
  query_id: string | null;
}

interface ChromeState {
  url: string;
  loading: boolean;
  closed_count: number;
  can_new_tab: boolean;
  width_preset: string;
}

const CHIP_MIN = 96;
const CHIP_MAX = 200;
const CHIP_GAP = 4;

let destinations: Destination[] = [];
let pages: PageInfo[] = [];
let activePageId: string | null = null;
let state: ChromeState = { url: "", loading: false, closed_count: 0, can_new_tab: false, width_preset: "medium" };
let editing = false;

const $ = (id: string) => document.getElementById(id)!;
const address = $("address");
const addrDisplay = $("addr-display");
const addrHost = $("addr-host");
const addrPath = $("addr-path");
const addrInput = $("addr-input") as HTMLInputElement;
const addrLock = $("addr-lock");
const loadLine = $("load-line");
const rowTabs = $("row-tabs");
const chips = $("chips");
const statusLive = $("status-live");

window.addEventListener("DOMContentLoaded", async () => {
  hydrateIcons();
  onLangChange(() => render());
  initI18n();
  installFaviconFallback(chips);
  applyMaterial(invoke);
  setupDom();
  setupListeners();
  await Promise.all([loadDestinations(), loadPages(), loadState()]);
  render();
});

// ─── Data ────────────────────────────────────────────────
async function loadDestinations() {
  destinations = await invoke<Destination[]>("get_destinations").catch(() => []);
}

async function loadPages() {
  pages = await invoke<PageInfo[]>("get_pages").catch(() => []);
  activePageId ??= pages.find((p) => p.state === "active")?.id ?? null;
}

async function loadState() {
  state = await invoke<ChromeState>("get_chrome_state").catch(() => state);
}

function activePage(): PageInfo | undefined {
  return pages.find((p) => p.id === activePageId);
}

function destTabs(): PageInfo[] {
  const a = activePage();
  return a ? pages.filter((p) => p.dest_id === a.dest_id) : [];
}

function destFor(p: PageInfo): Destination {
  return destinations.find((d) => d.id === p.dest_id) ?? { id: p.dest_id, name: p.dest_name, url: p.url, icon: p.dest_icon, order: 0 };
}

// ─── Render ──────────────────────────────────────────────
function render() {
  renderAddress();
  renderButtons();
  renderTabs();
}

function renderAddress() {
  const a = activePage();
  const raw = state.url || a?.url || "";
  loadLine.classList.toggle("hidden", !state.loading || !a);
  if (editing) return;
  if (!raw || !a) {
    addrLock.classList.add("hidden");
    addrHost.textContent = a ? "" : t("chrome.noPage");
    addrPath.textContent = "";
    return;
  }
  try {
    const u = new URL(raw);
    addrLock.classList.toggle("hidden", u.protocol !== "https:");
    addrHost.textContent = u.hostname.replace(/^www\./, "");
    const rest = decodeSafe(u.pathname + u.search);
    addrPath.textContent = rest === "/" ? "" : rest;
  } catch {
    addrLock.classList.add("hidden");
    addrHost.textContent = raw;
    addrPath.textContent = "";
  }
  addrDisplay.title = raw;
}

function decodeSafe(s: string): string {
  try {
    return decodeURI(s);
  } catch {
    return s;
  }
}

function renderButtons() {
  const a = activePage();
  const loaded = !!a && a.state !== "unloaded";
  for (const id of ["back-btn", "forward-btn", "reload-btn", "open-browser-btn", "save-btn"]) {
    ($(id) as HTMLButtonElement).disabled = !loaded;
  }
  const canNew = !!a && state.can_new_tab;
  for (const id of ["new-tab-btn", "tab-plus"]) {
    const b = $(id) as HTMLButtonElement;
    b.disabled = !canNew;
    b.title = a && !state.can_new_tab ? t("chrome.newTabFull") : t("chrome.newTab");
  }
}

function chipTitle(p: PageInfo): string {
  if (p.title.trim()) return p.title.trim();
  try {
    return new URL(p.url).hostname.replace(/^www\./, "") || p.dest_name;
  } catch {
    return p.dest_name;
  }
}

function renderTabs() {
  const tabs = destTabs();
  const show = tabs.length >= 2;
  rowTabs.classList.toggle("hidden", !show);
  chips.innerHTML = "";
  if (!show) return;
  for (const p of tabs) chips.appendChild(chip(p));
  sizeChips();
  chips.querySelector<HTMLElement>(".chip.selected")?.scrollIntoView({ block: "nearest", inline: "nearest" });
}

/** Chip width = clamp(available / count, 96, 200); the row scrolls once chips hit 96. */
function sizeChips() {
  const list = chips.querySelectorAll<HTMLElement>(".chip");
  if (!list.length) return;
  const avail = chips.clientWidth - CHIP_GAP * (list.length - 1);
  const w = Math.max(CHIP_MIN, Math.min(CHIP_MAX, Math.floor(avail / list.length)));
  list.forEach((c) => (c.style.width = `${w}px`));
}

function chip(p: PageInfo): HTMLElement {
  const el = document.createElement("div");
  const selected = p.id === activePageId;
  el.className = "chip";
  el.classList.toggle("selected", selected);
  el.classList.toggle("unloaded", p.state === "unloaded");
  el.classList.toggle("generating", p.generating);
  el.classList.toggle("unseen", !p.generating && p.unseen);
  el.setAttribute("role", "tab");
  el.setAttribute("aria-selected", String(selected));
  el.tabIndex = selected ? 0 : -1;
  const title = chipTitle(p);
  el.title = p.state === "unloaded" ? `${title}（${t("chrome.unloaded")}）` : title;

  const ic = document.createElement("span");
  ic.className = "chip-icon";
  ic.innerHTML = destTile(destFor(p), 16) + '<span class="gen-ring"></span><span class="unseen-dot"></span>';
  const label = document.createElement("span");
  label.className = "chip-title";
  label.textContent = title;
  const close = document.createElement("button");
  close.className = "chip-close";
  close.innerHTML = icon("sf-xmark", 12);
  close.tabIndex = -1; // Delete on the focused chip closes it from the keyboard
  close.title = t("chrome.closeTab");
  close.setAttribute("aria-label", t("chrome.closeNamed", { name: title }));
  close.addEventListener("click", (e) => {
    e.stopPropagation();
    invoke("close_page", { pageId: p.id }).catch(() => {});
  });
  el.append(ic, label, close);

  el.addEventListener("click", () => {
    if (p.id !== activePageId) invoke("switch_page", { pageId: p.id }).catch(() => {});
  });
  el.addEventListener("auxclick", (e) => {
    if (e.button === 1) invoke("close_page", { pageId: p.id }).catch(() => {});
  });
  el.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    invoke("show_tab_menu", { pageId: p.id }).catch(() => {});
  });
  el.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      invoke("switch_page", { pageId: p.id }).catch(() => {});
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      invoke("close_page", { pageId: p.id }).catch(() => {});
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const next = (e.key === "ArrowRight" ? el.nextElementSibling : el.previousElementSibling) as HTMLElement | null;
      next?.focus();
    }
  });
  return el;
}

// ─── Address editing ─────────────────────────────────────
function startEditing() {
  const a = activePage();
  if (!a || editing) return;
  editing = true;
  address.classList.add("editing");
  addrDisplay.classList.add("hidden");
  addrLock.classList.add("hidden");
  addrInput.classList.remove("hidden");
  addrInput.value = state.url || a.url || "";
  addrInput.focus();
  addrInput.select();
}

function stopEditing() {
  if (!editing) return;
  editing = false;
  address.classList.remove("editing");
  addrInput.classList.add("hidden");
  addrDisplay.classList.remove("hidden");
  renderAddress();
}

// ─── Wiring ──────────────────────────────────────────────
function flash(el: HTMLElement, kind: "ok" | "error") {
  el.classList.remove("flash-ok", "flash-error");
  void el.offsetWidth;
  el.classList.add(kind === "ok" ? "flash-ok" : "flash-error");
  setTimeout(() => el.classList.remove("flash-ok", "flash-error"), 1600);
}

function announce(msg: string) {
  statusLive.textContent = msg;
}

function on(id: string, fn: () => void) {
  $(id).addEventListener("click", fn);
}

function newTab() {
  const a = activePage();
  if (a) invoke("new_tab", { id: a.dest_id }).catch(() => {});
}

function setupDom() {
  address.addEventListener("click", startEditing);
  const addrComposing = trackComposition(addrInput);
  addrInput.addEventListener("keydown", (e) => {
    // Enter / Escape that confirm or cancel an IME composition are not ours.
    if (isImeKey(e) || addrComposing()) return;
    if (e.key === "Enter") {
      e.preventDefault();
      const value = addrInput.value;
      stopEditing();
      invoke("chrome_navigate", { input: value }).catch((err) => announce(String(err)));
    } else if (e.key === "Escape") {
      e.preventDefault();
      stopEditing();
    }
  });
  addrInput.addEventListener("blur", stopEditing);

  on("open-browser-btn", () => invoke("open_active_in_browser").catch(() => {}));
  on("back-btn", () => invoke("go_back").catch(() => {}));
  on("forward-btn", () => invoke("go_forward").catch(() => {}));
  on("reload-btn", () => invoke("reload_active_page").catch(() => {}));
  on("new-tab-btn", newTab);
  on("tab-plus", newTab);
  on("more-btn", () => {
    const r = $("more-btn").getBoundingClientRect();
    invoke("show_chrome_menu", { x: r.left, y: r.bottom + 4 }).catch(() => {});
  });

  const saveBtn = $("save-btn");
  on("save-btn", async () => {
    try {
      const r = await invoke<{ capture_status: string }>("save_answer");
      const msg = r.capture_status === "partial" ? t("side.savedPartial") : t("side.saved");
      saveBtn.title = msg;
      announce(msg);
      flash(saveBtn, "ok");
    } catch (e) {
      const msg = String(e);
      saveBtn.title = msg;
      announce(msg);
      flash(saveBtn, "error");
    }
    setTimeout(() => (saveBtn.title = t("side.save")), 4000);
  });

  // ⌘L edits the address; the rest are shared with the rail and the pages.
  document.addEventListener("keydown", (e) => {
    if (e.metaKey && !e.shiftKey && e.key.toLowerCase() === "l") {
      e.preventDefault();
      startEditing();
    }
  });
  installTabShortcuts({
    activePageId: () => activePageId,
    activeDestId: () => activePage()?.dest_id ?? null,
  });

  // SPA navigations don't fire page-load events: re-read URL/title on hover.
  let lastRefresh = 0;
  document.documentElement.addEventListener("mouseenter", () => {
    const now = Date.now();
    if (now - lastRefresh < 1000) return;
    lastRefresh = now;
    invoke("refresh_chrome").catch(() => {});
  });

  new ResizeObserver(() => sizeChips()).observe(chips);
}

function setupListeners() {
  listen<PageInfo[]>("pages-updated", async (e) => {
    pages = e.payload;
    await loadState();
    render();
  }).catch(() => {});
  listen<string>("active-page-changed", async (e) => {
    const next = e.payload || null;
    if (next !== activePageId) {
      activePageId = next;
      stopEditing();
      await loadState();
    }
    render();
  }).catch(() => {});
  listen("destinations-changed", async () => {
    await loadDestinations();
    render();
  }).catch(() => {});
  listen<{ label: string; loading: boolean; url: string }>("chrome-load", (e) => {
    const a = activePage();
    if (!a || a.label !== e.payload.label) return;
    state.loading = e.payload.loading;
    if (e.payload.url) state.url = e.payload.url;
    renderAddress();
  }).catch(() => {});
  listen("chrome-state-changed", async () => {
    await loadState();
    render();
  }).catch(() => {});
  listen<string>("notice", (e) => {
    announce(e.payload);
    const save = $("save-btn");
    save.title = e.payload;
    setTimeout(() => (save.title = t("side.save")), 4000);
  }).catch(() => {});
}
