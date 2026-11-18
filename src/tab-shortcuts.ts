// Tab / navigation shortcuts shared by the rail (main.ts) and the top chrome
// (chrome.ts). The page windows get the same set from the injected script in
// src-tauri/src/panel/mod.rs.
import { invoke } from "@tauri-apps/api/core";

export interface ShortcutContext {
  activePageId: () => string | null;
  activeDestId: () => string | null;
}

/** Install ⌘R ⌘W ⌘[ ⌘] ⌘N ⌘T ⌘⇧T ⌘⇧[ ⌘⇧] on `document`. */
export function installTabShortcuts(ctx: ShortcutContext): void {
  document.addEventListener("keydown", (e) => {
    if (!e.metaKey || e.altKey || e.ctrlKey) return;
    const key = e.key.toLowerCase();
    const run = (cmd: string, args?: Record<string, unknown>) => {
      e.preventDefault();
      invoke(cmd, args).catch(() => {});
    };
    if (e.shiftKey) {
      if (key === "t") run("reopen_closed_tab");
      else if (e.code === "BracketLeft") run("cycle_tab", { delta: -1 });
      else if (e.code === "BracketRight") run("cycle_tab", { delta: 1 });
      return;
    }
    if (key === "r") run("reload_active_page");
    else if (key === "w") {
      const id = ctx.activePageId();
      if (id) run("close_page", { pageId: id });
      else e.preventDefault();
    } else if (e.code === "BracketLeft") run("go_back");
    else if (e.code === "BracketRight") run("go_forward");
    else if (key === "n" || key === "t") {
      const dest = ctx.activeDestId();
      if (dest) run("new_tab", { id: dest });
      else e.preventDefault();
    }
  });
}
