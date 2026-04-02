import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Destination {
  id: string;
  name: string;
  url: string;
  icon: string;
  clip_prompt: string;
}

interface PickerData {
  destinations: Destination[];
  text: string;
}

let autoDismissTimer: ReturnType<typeof setTimeout> | null = null;
let cursorVisited = false;

async function refreshPicker() {
  try {
    const data = await invoke<PickerData>("get_picker_data");
    if (data.destinations.length > 0) {
      cursorVisited = false;
      renderPicker(data);
      // Long fallback timeout (15s) in case cursor never visits the picker
      scheduleAutoDismiss(15000);
    }
  } catch (e) {
    console.error("get_picker_data failed:", e);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  // Primary: page visibility change fires when NSPanel is shown/hidden
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      refreshPicker();
    }
  });

  // Backup: Tauri event from backend (catches cases where visibility doesn't fire)
  listen("show-picker", () => {
    refreshPicker();
  }).catch(console.error);

  // Close button
  document.getElementById("picker-close")!.addEventListener("click", dismissPicker);

  // Keyboard dismiss (may not fire if picker is non-activating, close button is the primary way)
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") dismissPicker();
  });

  // Stay visible until cursor visits then leaves
  const card = document.getElementById("picker-card")!;
  card.addEventListener("mouseenter", () => {
    cursorVisited = true;
    clearAutoDismiss();
  });
  card.addEventListener("mouseleave", () => {
    if (cursorVisited) {
      dismissPicker();
    }
  });
});

function renderPicker(data: PickerData) {
  const list = document.getElementById("picker-list")!;
  list.innerHTML = "";

  data.destinations.forEach((dest) => {
    const btn = document.createElement("button");
    btn.className = "picker-btn";
    const iconHtml = dest.icon && dest.icon.trim()
      ? `<span class="picker-icon">${dest.icon}</span>`
      : (() => { try { return `<img src="https://www.google.com/s2/favicons?domain=${new URL(dest.url).hostname}&sz=64" width="28" height="28" class="picker-icon-img" onerror="this.replaceWith(document.createTextNode('🌐'))">`; } catch { return `<span class="picker-icon">🌐</span>`; } })();
    btn.innerHTML = `
      ${iconHtml}
      <span class="picker-name">${dest.name}</span>
    `;
    btn.addEventListener("click", async () => {
      clearAutoDismiss();
      try {
        await invoke("pick_destination", {
          id: dest.id,
          text: data.text,
        });
      } catch (e) {
        console.error("pick_destination failed:", e);
      }
    });
    list.appendChild(btn);
  });
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
