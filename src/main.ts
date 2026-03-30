import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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
  label: string;
}

// ─── State ──────────────────────────────────────────────
let destinations: Destination[] = [];
let pages: PageInfo[] = [];
let activePageId: string | null = null;

const DEFAULT_DESTINATIONS: Destination[] = [
  { id: "google", name: "Google", url: "https://www.google.com", icon: "", order: 0 },
  { id: "chatgpt", name: "ChatGPT", url: "https://chat.openai.com", icon: "", order: 1 },
  { id: "claude", name: "Claude", url: "https://claude.ai", icon: "", order: 2 },
  { id: "gemini", name: "Gemini", url: "https://gemini.google.com", icon: "", order: 3 },
  { id: "perplexity", name: "Perplexity", url: "https://www.perplexity.ai", icon: "", order: 4 },
];


/** Get favicon URL for a destination. Returns Google's favicon service URL. */
function faviconUrl(url: string, size = 64): string {
  try {
    const domain = new URL(url).hostname;
    return `https://www.google.com/s2/favicons?domain=${domain}&sz=${size}`;
  } catch {
    return "";
  }
}

/** Render an icon element: if emoji is set use it, otherwise use website favicon */
function renderIcon(dest: { icon: string; url: string }, size = 24): string {
  // If icon is non-empty, use the emoji directly
  if (dest.icon && dest.icon.trim()) {
    return `<span>${dest.icon}</span>`;
  }
  // No emoji set — use favicon from the website
  const fav = faviconUrl(dest.url, size * 2); // 2x for retina
  if (fav) {
    return `<img src="${fav}" width="${size}" height="${size}" class="tab-favicon" onerror="this.replaceWith(document.createTextNode('🌐'))" alt="">`;
  }
  return `<span>🌐</span>`;
}

// ─── DOM refs ───────────────────────────────────────────
const tabList = document.getElementById("tab-list")!;
const settingsBtn = document.getElementById("settings-btn")!;

// ─── Init ───────────────────────────────────────────────
window.addEventListener("DOMContentLoaded", async () => {
  await loadDestinations();
  await loadPages();
  setupEventListeners();
  setupTauriListeners();
});

async function loadDestinations() {
  try {
    destinations = await invoke<Destination[]>("get_destinations");
  } catch (_e) {
    destinations = DEFAULT_DESTINATIONS;
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
    tabList.appendChild(sep);
  }

  systemDests.forEach((dest) => renderDestItem(dest));
}

function renderDestItem(dest: Destination) {
  const btn = document.createElement("button");
  btn.className = "tab-btn";
  const destPages = pages.filter((p) => p.dest_id === dest.id);
  const hasActivePage = destPages.some((p) => p.id === activePageId);
  if (hasActivePage) btn.classList.add("active");

  btn.dataset.id = dest.id;
  btn.innerHTML = `
    ${renderIcon(dest)}
    <span class="tab-tooltip">${dest.name}</span>
  `;
  btn.addEventListener("click", () => {
    if (dest.url.includes("system://calendar")) {
      invoke("open_system_app", { appName: "Calendar" });
      return;
    }
    if (dest.url.includes("system://reminders")) {
      invoke("open_system_app", { appName: "Reminders" });
      return;
    }
    switchDestination(dest.id);
  });

  tabList.appendChild(btn);

  // Page sub-tabs
  if (destPages.length > 0) {
    const pageGroup = document.createElement("div");
    pageGroup.className = "page-group";
    destPages.forEach((page, idx) => {
      const dotWrap = document.createElement("div");
      dotWrap.className = "page-dot-wrap";

      const dot = document.createElement("button");
      dot.className = "page-dot" + (page.id === activePageId ? " active" : "");
      dot.textContent = String(idx + 1);
      dot.title = `${page.dest_name} #${idx + 1}`;
      dot.addEventListener("click", (e) => {
        e.stopPropagation();
        switchPage(page.id);
      });
      dot.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        closePage(page.id);
      });

      const closeBtn = document.createElement("button");
      closeBtn.className = "page-close-btn";
      closeBtn.textContent = "✕";
      closeBtn.title = "Close tab";
      closeBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        closePage(page.id);
      });

      dotWrap.appendChild(dot);
      dotWrap.appendChild(closeBtn);
      pageGroup.appendChild(dotWrap);
    });
    tabList.appendChild(pageGroup);
  }
}

async function switchDestination(id: string) {
  try {
    await invoke("switch_destination", { id });
  } catch (_e) {}
}

async function switchPage(pageId: string) {
  try {
    await invoke("switch_page", { pageId });
  } catch (_e) {}
}

async function closePage(pageId: string) {
  try {
    await invoke("close_page", { pageId });
  } catch (_e) {}
}

// ─── Event listeners ─────────────────────────────────────
function setupEventListeners() {
  settingsBtn.addEventListener("click", async () => {
    try { await invoke("open_settings_window"); } catch (_e) {}
  });
  // Pin button — toggle auto-hide
  const pinBtn = document.getElementById("pin-btn");
  if (pinBtn) {
    // Initialize pin state
    invoke<boolean>("is_pinned").then((pinned) => {
      pinBtn.classList.toggle("pinned", pinned);
    }).catch(() => {});

    pinBtn.addEventListener("click", async () => {
      try {
        const pinned = await invoke<boolean>("toggle_pin");
        pinBtn.classList.toggle("pinned", pinned);
      } catch (_e) {}
    });
  }
  // New tab button
  document.getElementById("new-tab-btn")?.addEventListener("click", async () => {
    const activePage = pages.find((p) => p.id === activePageId);
    if (activePage) {
      try { await invoke("new_tab", { id: activePage.dest_id }); } catch (_e) {}
    }
  });
  // Open in default browser button
  document.getElementById("open-browser-btn")?.addEventListener("click", async () => {
    try { await invoke("open_active_in_browser"); } catch (_e) {}
  });
  // Reload button
  const reloadBtn = document.getElementById("reload-btn");
  if (reloadBtn) {
    reloadBtn.addEventListener("click", async () => {
      try { await invoke("reload_active_page"); } catch (_e) {}
    });
  }
  // Cmd+R to reload, Cmd+W to close active tab
  document.addEventListener("keydown", async (e) => {
    if (e.metaKey && e.key === "r") {
      e.preventDefault();
      try { await invoke("reload_active_page"); } catch (_e) {}
    } else if (e.metaKey && e.key === "w") {
      e.preventDefault();
      if (activePageId) {
        try { await invoke("close_page", { pageId: activePageId }); } catch (_e) {}
      }
    } else if (e.metaKey && e.key === "n") {
      e.preventDefault();
      // Open a new tab for the active destination
      const activePage = pages.find((p) => p.id === activePageId);
      if (activePage) {
        try { await invoke("new_tab", { id: activePage.dest_id }); } catch (_e) {}
      }
    }
  });
  // Width preset buttons
  document.querySelectorAll<HTMLButtonElement>(".width-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const preset = btn.dataset.width ?? "medium";
      document.querySelectorAll(".width-btn").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      try {
        await invoke("set_viewer_width", { preset });
      } catch (_e) {}
    });
  });
}

// ─── Tauri event listeners ───────────────────────────────
function setupTauriListeners() {
  listen("open-settings", async () => {
    try { await invoke("open_settings_window"); } catch (_e) {}
  }).catch(() => {});

  // Pages updated from backend
  listen<PageInfo[]>("pages-updated", (event) => {
    pages = event.payload;
    renderTabBar();
  }).catch(() => {});

  // Active page changed
  listen<string>("active-page-changed", (event) => {
    activePageId = event.payload;
    renderTabBar();
  }).catch(() => {});

  // Destinations changed (from settings window)
  listen("destinations-changed", async () => {
    await loadDestinations();
  }).catch(() => {});
}
