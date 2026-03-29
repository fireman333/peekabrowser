import { invoke } from "@tauri-apps/api/core";

interface SystemConfigData {
  item_type: string; // "calendar" or "reminders"
  text: string;
  lists: string[];
  needs_ocr: boolean;
}

const titleEl = document.getElementById("config-title")!;
const textInput = document.getElementById("config-text") as HTMLInputElement;
const listSelect = document.getElementById("config-list") as HTMLSelectElement;
const startInput = document.getElementById("config-start") as HTMLInputElement;
const endInput = document.getElementById("config-end") as HTMLInputElement;
const startGroup = document.getElementById("start-group")!;
const endGroup = document.getElementById("end-group")!;
const createBtn = document.getElementById("create-btn") as HTMLButtonElement;
const cancelBtn = document.getElementById("cancel-btn") as HTMLButtonElement;

let configData: SystemConfigData | null = null;

window.addEventListener("DOMContentLoaded", async () => {
  try {
    configData = await invoke<SystemConfigData>("get_system_config_data");
    setupUI(configData);

    // If OCR is needed, run it asynchronously and fill the title when done
    if (configData.needs_ocr) {
      textInput.placeholder = "Running OCR...";
      textInput.disabled = true;
      try {
        const ocrText = await invoke<string>("run_ocr");
        if (ocrText && ocrText.trim()) {
          textInput.value = ocrText.trim();
        } else {
          textInput.placeholder = "OCR returned empty — type manually";
        }
      } catch (e: any) {
        console.error("OCR failed:", e);
        textInput.placeholder = `OCR failed: ${e} — type manually`;
      } finally {
        textInput.disabled = false;
        textInput.focus();
      }
    }
  } catch (e) {
    console.error("get_system_config_data failed:", e);
  }

  createBtn.addEventListener("click", handleCreate);
  cancelBtn.addEventListener("click", handleCancel);
});

function setupUI(data: SystemConfigData) {
  textInput.value = data.text;

  if (data.item_type === "calendar") {
    titleEl.textContent = "New Calendar Event";
    document.querySelector('label[for="config-list"]')!.textContent = "Calendar";
    startGroup.classList.remove("hidden");
    endGroup.classList.remove("hidden");

    // Default start: now (rounded to next 15 min)
    const now = new Date();
    now.setMinutes(Math.ceil(now.getMinutes() / 15) * 15, 0, 0);
    startInput.value = toLocalDatetime(now);

    // Default end: +1 hour
    const end = new Date(now.getTime() + 3600000);
    endInput.value = toLocalDatetime(end);

    // Sync end when start changes
    startInput.addEventListener("change", () => {
      const s = new Date(startInput.value);
      if (!isNaN(s.getTime())) {
        endInput.value = toLocalDatetime(new Date(s.getTime() + 3600000));
      }
    });
  } else {
    // Reminders
    titleEl.textContent = "New Reminder";
    document.querySelector('label[for="config-list"]')!.textContent = "List";
    // Show start as optional due date, hide end
    document.querySelector('label[for="config-start"]')!.textContent = "Due Date (optional)";
    startInput.value = "";
    endGroup.classList.add("hidden");
  }

  // Populate list dropdown
  listSelect.innerHTML = "";
  data.lists.forEach((name) => {
    const opt = document.createElement("option");
    opt.value = name;
    opt.textContent = name;
    listSelect.appendChild(opt);
  });
}

function toLocalDatetime(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

async function handleCreate() {
  if (!configData) return;
  const text = textInput.value.trim();
  if (!text) {
    textInput.focus();
    return;
  }

  createBtn.disabled = true;
  createBtn.textContent = "Creating...";

  try {
    await invoke("create_system_item", {
      itemType: configData.item_type,
      text,
      listName: listSelect.value,
      startTime: startInput.value || null,
      endTime: endInput.value || null,
    });
  } catch (e) {
    console.error("create_system_item failed:", e);
    alert("Failed: " + e);
    createBtn.disabled = false;
    createBtn.textContent = "Create";
    return;
  }

  // Close window
  try {
    await invoke("close_system_config");
  } catch (_e) {}
}

async function handleCancel() {
  try {
    await invoke("close_system_config");
  } catch (_e) {}
}
