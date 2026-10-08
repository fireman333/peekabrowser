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

/** Favicon (or emoji) for a destination, with a line-icon fallback. */
export function destIcon(dest: { icon: string; url: string }, size = 24): string {
  if (dest.icon && dest.icon.trim()) return `<span class="emoji-icon" aria-hidden="true">${escapeHtml(dest.icon)}</span>`;
  try {
    const domain = new URL(dest.url).hostname;
    if (domain) {
      return `<img src="https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=${size * 2}" width="${size}" height="${size}" class="tab-favicon" alt="" data-fallback="globe">`;
    }
  } catch {
    /* fall through */
  }
  return icon("globe", size - 4);
}

/** Replace broken favicons with the globe icon (CSP-friendly; no inline onerror). */
export function installFaviconFallback(root: HTMLElement): void {
  root.addEventListener(
    "error",
    (e) => {
      const img = e.target as HTMLElement;
      if (img.tagName === "IMG" && img.dataset.fallback) {
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
