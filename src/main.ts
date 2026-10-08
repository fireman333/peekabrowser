import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { applyMaterial, destIcon, hydrateIcons, installFaviconFallback } from "./icons";
import { initI18n, onLangChange, t } from "./i18n";

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
  title: string;
  url: string;
  query_id: string | null;
}

// ─── State ──────────────────────────────────────────────
let destinations: Destination[] = [];
let pages: PageInfo[] = [];
let activePageId: string | null = null;

const tabList = document.getElementById("tab-list")!;
const statusLive = document.getElementById("status-live")!;

window.addEventListener("DOMContentLoaded", async () => {
  hydrateIcons();
  await initI18n();
  onLangChange(() => {
    renderTabBar();
    markUpdate(lastUpdate);
  });
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
  renderTabBar();
}

async function loadPages() {
  try {
    pages = await invoke<PageInfo[]>("get_pages");
  } catch (_e) {
    pages = [];
  }
  renderTabBar();
}

// ─── Tab Bar ────────────────────────────────────────────
function isSystemDest(dest: Destination): boolean {
  return dest.url.includes("system://");
}

function renderTabBar() {
  tabList.innerHTML = "";
  const sorted = [...destinations].sort((a, b) => a.order - b.order);
  const regularDests = sorted.filter((d) => !isSystemDest(d));
  const systemDests = sorted.filter((d) => isSystemDest(d));

  regularDests.forEach((dest) => renderDestItem(dest));

  if (systemDests.length > 0 && regularDests.length > 0) {
    const sep = document.createElement("div");
    sep.className = "system-separator";
    sep.setAttribute("role", "separator");
    tabList.appendChild(sep);
  }
  systemDests.forEach((dest) => renderDestItem(dest));
  updateActionStates();
}

function pageLabel(page: PageInfo, idx: number): string {
  const title = page.title?.trim() || `${page.dest_name} #${idx + 1}`;
  const state = page.generating
    ? t("side.stGenerating")
    : page.state === "unloaded"
      ? t("side.stUnloaded")
      : page.id === activePageId
        ? t("side.stCurrent")
        : t("side.stIdle");
  return `${title} (${state})`;
}

function renderDestItem(dest: Destination) {
  const btn = document.createElement("button");
  btn.className = "tab-btn dest-btn";
  btn.setAttribute("role", "listitem");
  const destPages = pages.filter((p) => p.dest_id === dest.id);
  const hasActivePage = destPages.some((p) => p.id === activePageId);
  if (hasActivePage) {
    btn.classList.add("active");
    btn.setAttribute("aria-current", "page");
  }
  if (destPages.some((p) => p.generating)) btn.classList.add("generating");

  btn.dataset.id = dest.id;
  btn.title = dest.name;
  btn.setAttribute("aria-label", destPages.length ? t("side.pages", { name: dest.name, n: destPages.length }) : dest.name);
  btn.innerHTML = destIcon(dest);
  btn.addEventListener("click", () => {
    if (dest.url.includes("system://calendar")) {
      invoke("open_system_app", { appName: "Calendar" });
      return;
    }
    if (dest.url.includes("system://reminders")) {
      invoke("open_system_app", { appName: "Reminders" });
      return;
    }
    invoke("switch_destination", { id: dest.id }).catch(() => {});
  });
  tabList.appendChild(btn);

  if (destPages.length === 0) return;
  const pageGroup = document.createElement("div");
  pageGroup.className = "page-group";
  pageGroup.setAttribute("role", "group");
  pageGroup.setAttribute("aria-label", t("side.pagesGroup", { name: dest.name }));
  destPages.forEach((page, idx) => {
    const wrap = document.createElement("div");
    wrap.className = "page-dot-wrap";

    const dot = document.createElement("button");
    const classes = ["page-dot"];
    if (page.id === activePageId) classes.push("active");
    if (page.state === "unloaded") classes.push("unloaded");
    if (page.generating) classes.push("generating");
    dot.className = classes.join(" ");
    dot.textContent = String(idx + 1);
    const label = pageLabel(page, idx);
    dot.title = label;
    dot.setAttribute("aria-label", label);
    if (page.id === activePageId) dot.setAttribute("aria-current", "page");
    dot.addEventListener("click", (e) => {
      e.stopPropagation();
      invoke("switch_page", { pageId: page.id }).catch(() => {});
    });
    dot.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      closePage(page.id);
    });
    dot.addEventListener("keydown", (e) => {
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        closePage(page.id);
      }
    });

    const closeBtn = document.createElement("button");
    closeBtn.className = "page-close-btn";
    closeBtn.innerHTML = "×";
    closeBtn.title = t("side.closePage");
    closeBtn.setAttribute("aria-label", t("side.closeNamed", { name: page.title || page.dest_name }));
    closeBtn.tabIndex = -1; // Delete on the focused dot closes it from the keyboard
    closeBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      closePage(page.id);
    });

    wrap.appendChild(dot);
    wrap.appendChild(closeBtn);
    pageGroup.appendChild(wrap);
  });
  tabList.appendChild(pageGroup);
}

async function closePage(pageId: string) {
  try {
    await invoke("close_page", { pageId });
  } catch (_e) {}
}

/** Disable actions that need a loaded page. */
function updateActionStates() {
  const active = pages.find((p) => p.id === activePageId);
  const loaded = !!active && active.state !== "unloaded";
  for (const id of ["save-btn", "back-btn", "forward-btn", "reload-btn", "open-browser-btn", "new-tab-btn"]) {
    const el = document.getElementById(id) as HTMLButtonElement | null;
    if (el) el.disabled = id === "new-tab-btn" ? !active : !loaded;
  }
}

function flash(el: HTMLElement | null, kind: "ok" | "error") {
  if (!el) return;
  el.classList.remove("flash-ok", "flash-error");
  void el.offsetWidth;
  el.classList.add(kind === "ok" ? "flash-ok" : "flash-error");
  setTimeout(() => el.classList.remove("flash-ok", "flash-error"), 1600);
}

function announce(msg: string) {
  statusLive.textContent = msg;
}

// ─── Event listeners ─────────────────────────────────────
function on(id: string, fn: () => void) {
  document.getElementById(id)?.addEventListener("click", fn);
}

function setupEventListeners() {
  on("settings-btn", () => invoke("open_settings_window").catch(() => {}));
  on("records-btn", () => invoke("open_records_window").catch(() => {}));
  on("back-btn", () => invoke("go_back").catch(() => {}));
  on("forward-btn", () => invoke("go_forward").catch(() => {}));
  on("reload-btn", () => invoke("reload_active_page").catch(() => {}));
  on("open-browser-btn", () => invoke("open_active_in_browser").catch(() => {}));
  on("capture-btn", () => invoke("take_screenshot").catch(() => {}));
  on("new-tab-btn", () => {
    const activePage = pages.find((p) => p.id === activePageId);
    if (activePage) invoke("new_tab", { id: activePage.dest_id }).catch(() => {});
  });

  const saveBtn = document.getElementById("save-btn");
  on("save-btn", async () => {
    try {
      const r = await invoke<{ capture_status: string }>("save_answer");
      const msg = r.capture_status === "partial" ? t("side.savedPartial") : t("side.saved");
      saveBtn!.title = msg;
      announce(msg);
      flash(saveBtn, "ok");
    } catch (e) {
      const msg = String(e);
      saveBtn!.title = msg;
      announce(msg);
      flash(saveBtn, "error");
    }
    setTimeout(() => (saveBtn!.title = t("side.save")), 4000);
  });

  const moreBtn = document.getElementById("more-btn")!;
  const moreGroup = document.getElementById("more-group")!;
  moreBtn.addEventListener("click", () => {
    const open = moreGroup.classList.toggle("hidden") === false;
    moreBtn.setAttribute("aria-expanded", String(open));
    moreBtn.classList.toggle("active", open);
  });

  const pinBtn = document.getElementById("pin-btn")!;
  const setPinned = (pinned: boolean) => {
    pinBtn.classList.toggle("pinned", pinned);
    pinBtn.setAttribute("aria-pressed", String(pinned));
  };
  invoke<boolean>("is_pinned").then(setPinned).catch(() => {});
  pinBtn.addEventListener("click", async () => {
    try {
      setPinned(await invoke<boolean>("toggle_pin"));
    } catch (_e) {}
  });

  document.addEventListener("keydown", async (e) => {
    if (!e.metaKey) return;
    const activePage = pages.find((p) => p.id === activePageId);
    if (e.key === "r") {
      e.preventDefault();
      invoke("reload_active_page").catch(() => {});
    } else if (e.key === "w") {
      e.preventDefault();
      if (activePageId) closePage(activePageId);
    } else if (e.key === "[") {
      e.preventDefault();
      invoke("go_back").catch(() => {});
    } else if (e.key === "]") {
      e.preventDefault();
      invoke("go_forward").catch(() => {});
    } else if (e.key === "n") {
      e.preventDefault();
      if (activePage) invoke("new_tab", { id: activePage.dest_id }).catch(() => {});
    }
  });

  document.querySelectorAll<HTMLButtonElement>(".width-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const preset = btn.dataset.width ?? "medium";
      document.querySelectorAll(".width-btn").forEach((b) => {
        b.classList.remove("active");
        b.setAttribute("aria-checked", "false");
      });
      btn.classList.add("active");
      btn.setAttribute("aria-checked", "true");
      invoke("set_viewer_width", { preset }).catch(() => {});
    });
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
    renderTabBar();
  }).catch(() => {});

  listen<string>("active-page-changed", (event) => {
    activePageId = event.payload || null;
    renderTabBar();
  }).catch(() => {});

  listen("destinations-changed", () => loadDestinations()).catch(() => {});

  // Badge the settings button when an update is available.
  invoke<UpdateBadge>("get_update_status").then(markUpdate).catch(() => {});
  listen<UpdateBadge>("update-status", (e) => markUpdate(e.payload)).catch(() => {});

  listen<string>("notice", (event) => {
    announce(event.payload);
    const save = document.getElementById("save-btn");
    if (save) {
      save.title = event.payload;
      setTimeout(() => (save.title = t("side.save")), 4000);
    }
  }).catch(() => {});
}
