// The rail (sidebar window): destinations, then system destinations after a
// divider, then Records / Pin / Settings. Navigation, the address bar and the
// tab row live in the top chrome window (chrome.ts).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { applyMaterial, destTile, hydrateIcons, installFaviconFallback } from "./icons";
import { initI18n, onLangChange, t } from "./i18n";
import { installTabShortcuts } from "./tab-shortcuts";

interface Destination {
  id: string;
  name: string;
  url: string;
  icon: string;
  order: number;
}

type PageState = "active" | "background" | "unloaded";

interface PageInfo {
  id: string;
  dest_id: string;
  dest_name: string;
  dest_icon: string;
  label: string | null;
  state: PageState;
  generating: boolean;
  unseen: boolean;
  title: string;
  url: string;
  query_id: string | null;
}

// ─── State ──────────────────────────────────────────────
let destinations: Destination[] = [];
let pages: PageInfo[] = [];
let activePageId: string | null = null;
/** Selected destination (may have no tabs: the empty state). */
let activeDestId: string | null = null;
let pinned = false;

const tabList = document.getElementById("tab-list")!;
const statusLive = document.getElementById("status-live")!;

window.addEventListener("DOMContentLoaded", async () => {
  hydrateIcons();
  // Language loads in the background; the rail renders immediately with the
  // default strings and re-renders if the saved language differs.
  onLangChange(() => {
    renderRail();
    renderPin();
    markUpdate(lastUpdate);
  });
  initI18n();
  installFaviconFallback(tabList);
  applyMaterial(invoke);
  await loadDestinations();
  await loadPages();
  setupEventListeners();
  setupTauriListeners();
});

async function loadDestinations() {
  try {
    destinations = await invoke<Destination[]>("get_destinations");
  } catch (_e) {
    destinations = [];
  }
  renderRail();
}

async function loadPages() {
  try {
    pages = await invoke<PageInfo[]>("get_pages");
  } catch (_e) {
    pages = [];
  }
  renderRail();
}

// ─── Rail ───────────────────────────────────────────────
function isSystemDest(dest: Destination): boolean {
  return dest.url.includes("system://");
}

function renderRail() {
  tabList.innerHTML = "";
  const sorted = [...destinations].sort((a, b) => a.order - b.order);
  const regular = sorted.filter((d) => !isSystemDest(d));
  const system = sorted.filter((d) => isSystemDest(d));

  regular.forEach((dest) => tabList.appendChild(railItem(dest)));
  tabList.appendChild(addSiteItem());
  if (system.length > 0) {
    const sep = document.createElement("div");
    sep.className = "rail-divider";
    sep.setAttribute("role", "separator");
    tabList.appendChild(sep);
  }
  system.forEach((dest) => tabList.appendChild(railItem(dest)));
  tabList.querySelector(".rail-item.selected")?.scrollIntoView({ block: "nearest" });
}

/** "+" at the end of the sites (above Calendar / Reminders): Add a site. */
function addSiteItem(): HTMLElement {
  const btn = document.createElement("button");
  btn.className = "rail-item rail-add";
  btn.setAttribute("role", "listitem");
  btn.setAttribute("aria-label", t("rail.addSite"));
  btn.title = t("rail.addSite");
  const iconWrap = document.createElement("span");
  iconWrap.className = "rail-icon";
  iconWrap.innerHTML = '<span class="add-tile" aria-hidden="true">+</span>';
  btn.append(iconWrap);
  btn.addEventListener("click", () => invoke("open_add_site").catch(() => {}));
  return btn;
}

/** Tab indicator: none for one tab, up to three dots, a number from four on. */
function indicator(count: number): HTMLElement {
  const row = document.createElement("span");
  row.className = "tab-indicator";
  row.setAttribute("aria-hidden", "true");
  if (count >= 4) {
    const badge = document.createElement("span");
    badge.className = "count-badge";
    badge.textContent = String(count);
    row.appendChild(badge);
  } else if (count >= 2) {
    for (let i = 0; i < count; i++) row.appendChild(document.createElement("i"));
  }
  return row;
}

function railItem(dest: Destination): HTMLElement {
  const btn = document.createElement("button");
  btn.className = "rail-item";
  btn.setAttribute("role", "listitem");
  btn.dataset.id = dest.id;
  const destPages = pages.filter((p) => p.dest_id === dest.id);
  const selected = dest.id === activeDestId;
  const generating = destPages.some((p) => p.generating);
  const unseen = !generating && destPages.some((p) => p.unseen);
  btn.classList.toggle("selected", selected);
  btn.classList.toggle("generating", generating);
  btn.classList.toggle("unseen", unseen);
  if (selected) btn.setAttribute("aria-current", "page");

  const parts = [destPages.length > 1 ? t("rail.tabs", { name: dest.name, n: destPages.length }) : dest.name];
  if (generating) parts.push(t("rail.generating"));
  else if (unseen) parts.push(t("rail.unseen"));
  btn.setAttribute("aria-label", parts.join("，"));
  btn.title = dest.name;

  const iconWrap = document.createElement("span");
  iconWrap.className = "rail-icon";
  iconWrap.innerHTML = destTile(dest, 28) + '<span class="gen-ring"></span><span class="unseen-dot"></span>';
  const name = document.createElement("span");
  name.className = "rail-name";
  name.textContent = dest.name;
  btn.append(iconWrap, name, indicator(destPages.length));

  btn.addEventListener("click", () => {
    if (dest.url.includes("system://calendar")) {
      invoke("open_system_app", { appName: "Calendar" });
      return;
    }
    if (dest.url.includes("system://reminders")) {
      invoke("open_system_app", { appName: "Reminders" });
      return;
    }
    // Shows the destination's most recent tab, or its empty state when it
    // has none; never opens a tab.
    invoke("switch_destination", { id: dest.id }).catch(() => {});
  });
  return btn;
}

function announce(msg: string) {
  statusLive.textContent = msg;
}

const pinBtn = document.getElementById("pin-btn")!;
function renderPin() {
  pinBtn.classList.toggle("pinned", pinned);
  pinBtn.setAttribute("aria-pressed", String(pinned));
  const label = pinned ? t("rail.unpin") : t("rail.pin");
  pinBtn.title = label;
  pinBtn.setAttribute("aria-label", label);
}

// ─── Event listeners ─────────────────────────────────────
function on(id: string, fn: () => void) {
  document.getElementById(id)?.addEventListener("click", fn);
}

function setupEventListeners() {
  on("settings-btn", () => invoke("open_settings_window").catch(() => {}));
  on("records-btn", () => invoke("open_records_window").catch(() => {}));

  invoke<boolean>("is_pinned")
    .then((p) => {
      pinned = p;
      renderPin();
    })
    .catch(() => {});
  pinBtn.addEventListener("click", async () => {
    try {
      pinned = await invoke<boolean>("toggle_pin");
      renderPin();
    } catch (_e) {}
  });

  installTabShortcuts({
    activePageId: () => activePageId,
    activeDestId: () => activeDestId,
  });
}

type UpdateBadge = { info: { available: boolean; latest: string } | null };
let lastUpdate: UpdateBadge | null = null;

function markUpdate(st: UpdateBadge | null) {
  lastUpdate = st;
  const btn = document.getElementById("settings-btn");
  const available = !!st?.info?.available;
  btn?.classList.toggle("has-update", available);
  if (btn) btn.title = available ? t("side.settingsUpdate", { v: st!.info!.latest }) : t("side.settings");
}

// ─── Tauri event listeners ───────────────────────────────
function setupTauriListeners() {
  listen("open-settings", () => invoke("open_settings_window").catch(() => {})).catch(() => {});

  listen<PageInfo[]>("pages-updated", (event) => {
    pages = event.payload;
    renderRail();
  }).catch(() => {});

  listen<string>("active-page-changed", (event) => {
    activePageId = event.payload || null;
  }).catch(() => {});

  listen<string>("active-dest-changed", (event) => {
    activeDestId = event.payload || null;
    renderRail();
  }).catch(() => {});

  listen("destinations-changed", () => loadDestinations()).catch(() => {});

  // Badge the settings button when an update is available.
  invoke<UpdateBadge>("get_update_status").then(markUpdate).catch(() => {});
  listen<UpdateBadge>("update-status", (e) => markUpdate(e.payload)).catch(() => {});

  listen<string>("notice", (event) => announce(event.payload)).catch(() => {});

  // Pin can also change from the chrome or elsewhere; re-read when focused.
  window.addEventListener("focus", () => {
    invoke<boolean>("is_pinned")
      .then((p) => {
        pinned = p;
        renderPin();
      })
      .catch(() => {});
  });
}
