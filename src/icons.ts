// One consistent line-icon set for app actions (brand favicons stay for services).
const PATHS: Record<string, string> = {
  save: '<path d="M6 3h9l3 3v15H6z"/><path d="M9 3v5h6"/><path d="M9 14h6M9 17h4"/>',
  records: '<path d="M4 6h16M4 12h16M4 18h10"/>',
  back: '<path d="M15 5l-7 7 7 7"/>',
  forward: '<path d="M9 5l7 7-7 7"/>',
  reload: '<path d="M20 12a8 8 0 1 1-2.3-5.6"/><path d="M20 4v5h-5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  capture: '<path d="M4 8V5h3M17 5h3v3M20 16v3h-3M7 19H4v-3"/><circle cx="12" cy="12" r="3"/>',
  external: '<path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M18 14v5H5V6h5"/>',
  more: '<circle cx="6" cy="12" r="1.2"/><circle cx="12" cy="12" r="1.2"/><circle cx="18" cy="12" r="1.2"/>',
  pin: '<path d="M9 4h6l-1 6 3 3H7l3-3z"/><path d="M12 13v7"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M5.6 18.4L7 17M17 7l1.4-1.4"/>',
  globe: '<circle cx="12" cy="12" r="8"/><path d="M4 12h16M12 4c2.5 2.5 2.5 13.5 0 16M12 4c-2.5 2.5-2.5 13.5 0 16"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  star: '<path d="M12 4l2.4 5 5.4.6-4 3.7 1.1 5.3L12 16l-4.9 2.6 1.1-5.3-4-3.7 5.4-.6z"/>',
  // SF Symbol-like line icons for the rail and the top chrome.
  "sf-clock": '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
  "sf-pin": '<path d="M14.6 3.8l5.6 5.6"/><path d="M17.4 6.6l-4.2 4.2-4-.7-1.9 1.9 6.7 6.7 1.9-1.9-.7-4 4.2-4.2"/><path d="M9.9 15.7l-5.4 5.4"/>',
  "sf-gear": '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>',
  "sf-lock": '<rect x="5.5" y="10.5" width="13" height="9.5" rx="2"/><path d="M8.5 10.5V7.8a3.5 3.5 0 0 1 7 0v2.7"/>',
  "sf-chevron-left": '<path d="M14.5 5.5L8 12l6.5 6.5"/>',
  "sf-chevron-right": '<path d="M9.5 5.5L16 12l-6.5 6.5"/>',
  "sf-arrow-clockwise": '<path d="M19 12a7 7 0 1 1-2.05-4.95"/><path d="M19.2 4.6v4.2H15"/>',
  "sf-save": '<path d="M12 4v10"/><path d="M8 10.5l4 4 4-4"/><path d="M5 14.5v4A1.5 1.5 0 0 0 6.5 20h11a1.5 1.5 0 0 0 1.5-1.5v-4"/>',
  "sf-ellipsis": '<circle cx="6" cy="12" r="1.4" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none"/><circle cx="18" cy="12" r="1.4" fill="currentColor" stroke="none"/>',
  "sf-plus": '<path d="M12 5v14M5 12h14"/>',
  "sf-safari": '<circle cx="12" cy="12" r="8.5"/><path d="M15.5 8.5l-2 5-5 2 2-5z"/>',
  "sf-xmark": '<path d="M7.5 7.5l9 9M16.5 7.5l-9 9"/>',
  "sf-text-quote": '<path d="M4.5 6.5v11"/><path d="M8.5 7h11M8.5 12h11M8.5 17h7"/>',
};

export function icon(name: string, size = 18): string {
  const p = PATHS[name] ?? PATHS.globe;
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${p}</svg>`;
}

/** Fill every `[data-icon]` element under `root` with its icon. */
export function hydrateIcons(root: ParentNode = document): void {
  root.querySelectorAll<HTMLElement>("[data-icon]").forEach((el) => {
    el.innerHTML = icon(el.dataset.icon!);
  });
}

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]!));
}

/** High-resolution site icon (apple-touch-icon quality when the site has one).
 * Google's s2 endpoint often returns a 16px favicon upscaled, which looks blurry. */
export function faviconUrl(domain: string, displaySize: number): string {
  const want = displaySize * 3;
  const px = want <= 64 ? 64 : want <= 128 ? 128 : 256;
  return `https://t1.gstatic.com/faviconV2?client=SOCIAL&type=FAVICON&fallback_opts=TYPE,SIZE,URL&url=${encodeURIComponent(`https://${domain}`)}&size=${px}`;
}

/** Favicon (or emoji) for a destination, with a line-icon fallback. */
export function destIcon(dest: { icon: string; url: string }, size = 24): string {
  if (dest.icon && dest.icon.trim()) return `<span class="emoji-icon" aria-hidden="true">${escapeHtml(dest.icon)}</span>`;
  try {
    const domain = new URL(dest.url).hostname;
    if (domain) {
      return `<img src="${faviconUrl(domain, size)}" width="${size}" height="${size}" class="tab-favicon" alt="" data-fallback="globe">`;
    }
  } catch {
    /* fall through */
  }
  return icon("globe", size - 4);
}

const TILE_KEYS = ["google", "chatgpt", "claude", "gemini", "perplexity", "openevidence"];

/** Monogram tile: rounded square (radius = side × 0.29) in the destination's tile colour, white initial. */
export function monogramTile(name: string, id: string, size: number): string {
  const key = TILE_KEYS.find((k) => id === k || name.toLowerCase().replace(/\s+/g, "") === k) ?? "other";
  const initial = escapeHtml((Array.from(name.trim())[0] ?? "?").toUpperCase());
  const r = Math.round(size * 0.29 * 10) / 10;
  return `<span class="mono-tile" aria-hidden="true" style="width:${size}px;height:${size}px;border-radius:${r}px;background:var(--tile-${key});font-size:${Math.round(size * 0.5)}px">${initial}</span>`;
}

/** Replace broken favicons with the globe icon (CSP-friendly; no inline onerror). */
export function installFaviconFallback(root: HTMLElement): void {
  root.addEventListener(
    "error",
    (e) => {
      const img = e.target as HTMLElement;
      if (img.tagName === "IMG" && img.dataset.fallback === "mono") {
        const span = document.createElement("span");
        span.innerHTML = monogramTile(img.dataset.name ?? "", img.dataset.id ?? "", Number(img.getAttribute("width")) || 28);
        img.replaceWith(span.firstElementChild ?? span);
      } else if (img.tagName === "IMG" && img.dataset.fallback) {
        const span = document.createElement("span");
        span.innerHTML = icon(img.dataset.fallback, 20);
        img.replaceWith(span);
      }
    },
    true,
  );
}

/** Apply the native-material marker so CSS can let the window material show through. */
export async function applyMaterial(invoke: <T>(cmd: string) => Promise<T>): Promise<void> {
  try {
    const kind = await invoke<string>("get_material_kind");
    if (kind) document.documentElement.dataset.material = kind;
  } catch {
    /* solid fallback */
  }
}

/** Rail / tab-chip icon: emoji, else the site favicon, else (or when it fails) a monogram tile. */
export function destTile(dest: { id: string; name: string; icon: string; url: string }, size: number): string {
  if (dest.icon && dest.icon.trim()) {
    return `<span class="emoji-icon" aria-hidden="true" style="font-size:${Math.round(size * 0.82)}px">${escapeHtml(dest.icon)}</span>`;
  }
  try {
    const domain = new URL(dest.url).hostname;
    if (domain && !dest.url.includes("system://")) {
      return `<img src="${faviconUrl(domain, size)}" width="${size}" height="${size}" class="tab-favicon" alt="" data-fallback="mono" data-name="${escapeHtml(dest.name)}" data-id="${escapeHtml(dest.id)}">`;
    }
  } catch {
    /* fall through */
  }
  return monogramTile(dest.name, dest.id, size);
}
