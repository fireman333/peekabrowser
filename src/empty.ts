// Empty state window: shown in place of the page when the selected
// destination has no tabs (its last tab was closed, or it was selected with
// none). New tab opens one in that destination; ⌘W does nothing here (it
// never hides the panel).
import { invoke } from "@tauri-apps/api/core";
import { applyMaterial, hydrateIcons } from "./icons";
import { initI18n } from "./i18n";

const newTab = () => invoke("new_tab_for_active").catch(() => {});

hydrateIcons();
initI18n();
applyMaterial(invoke);

document.getElementById("new-tab")!.addEventListener("click", newTab);

document.addEventListener("keydown", (e) => {
  if (!e.metaKey || e.altKey || e.ctrlKey) return;
  const key = e.key.toLowerCase();
  if (e.shiftKey) {
    if (key === "t") {
      e.preventDefault();
      invoke("reopen_closed_tab").catch(() => {});
    }
    return;
  }
  if (key === "t" || key === "n") {
    e.preventDefault();
    newTab();
  } else if (key === "w" || key === "r" || e.code === "BracketLeft" || e.code === "BracketRight") {
    e.preventDefault();
  }
});
