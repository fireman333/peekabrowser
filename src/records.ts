import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { escapeHtml, hydrateIcons, icon } from "./icons";
import { getLang, initI18n, onLangChange, t, type Key } from "./i18n";

interface RecordItem {
  id: string;
  created_at: number;
  updated_at: number;
  source_app: string | null;
  source_title: string | null;
  source_url: string | null;
  selection_text: string | null;
  attachment_path: string | null;
  action_id: string;
  destination_id: string;
  destination_name: string;
  prompt: string;
  conversation_url: string | null;
  response_text: string | null;
  response_markdown: string | null;
  citation_links: string[];
  capture_status: string | null;
  query_status: string;
  tags: string[];
  note: string | null;
  favorite: boolean;
}

const CAPTURE_LABEL: Record<string, Key> = {
  complete: "rec.capComplete",
  partial: "rec.capPartial",
  unknown: "rec.capUnknown",
  manual_selection: "rec.capSelection",
};

const STATUS_LABEL: Record<string, Key> = {
  saved: "rec.stSaved",
  queued: "rec.stQueued",
  injected: "rec.stInjected",
  generating: "rec.stGenerating",
  completed: "rec.stCompleted",
  failed: "rec.stFailed",
};

function statusLabel(s: string): string {
  const k = STATUS_LABEL[s];
  return k ? t(k) : s;
}

let records: RecordItem[] = [];
let selectedId: string | null = null;
let searchTimer: ReturnType<typeof setTimeout> | null = null;

const listEl = document.getElementById("record-list")!;
const detailEl = document.getElementById("detail")!;
const searchEl = document.getElementById("search") as HTMLInputElement;
const favEl = document.getElementById("fav-only") as HTMLInputElement;

window.addEventListener("DOMContentLoaded", async () => {
  hydrateIcons();
  await initI18n();
  onLangChange(() => {
    renderList();
    renderDetail();
  });
  searchEl.addEventListener("input", () => {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(load, 200);
  });
  favEl.addEventListener("change", load);
  listEl.addEventListener("keydown", (e) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const idx = records.findIndex((r) => r.id === selectedId);
    const next = records[Math.max(0, Math.min(records.length - 1, idx + (e.key === "ArrowDown" ? 1 : -1)))];
    if (next) select(next.id, true);
  });
  // Links open in the default browser, never inside this window.
  detailEl.addEventListener("click", (e) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") || "";
    if (/^https?:\/\//.test(href)) invoke("open_settings_url", { url: href }).catch(() => {});
  });
  listen("records-changed", load).catch(() => {});
  document.addEventListener("visibilitychange", () => { if (!document.hidden) load(); });
  await load();
});

async function load() {
  try {
    records = await invoke<RecordItem[]>("list_records", {
      query: searchEl.value || null,
      favoritesOnly: favEl.checked,
    });
  } catch (e) {
    records = [];
    detailEl.innerHTML = `<div class="empty"><p>${escapeHtml(t("rec.unavailable", { e: String(e) }))}</p></div>`;
  }
  if (!records.some((r) => r.id === selectedId)) selectedId = records[0]?.id ?? null;
  renderList();
  renderDetail();
}

function titleOf(r: RecordItem): string {
  const line = (r.selection_text || r.prompt || r.response_text || "").trim().split("\n")[0];
  if (line) return line.length > 80 ? line.slice(0, 80) + "…" : line;
  return r.attachment_path ? t("rec.screenshot") : t("rec.answerOf", { name: r.destination_name });
}

function when(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleString(getLang() === "en" ? "en-US" : "zh-TW", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function renderList() {
  listEl.innerHTML = "";
  if (records.length === 0) {
    listEl.innerHTML = `<li class="list-empty">${searchEl.value || favEl.checked ? t("rec.noMatch") : t("rec.none")}</li>`;
    return;
  }
  for (const r of records) {
    const li = document.createElement("li");
    li.className = "record-item" + (r.id === selectedId ? " selected" : "");
    li.setAttribute("role", "option");
    li.setAttribute("aria-selected", String(r.id === selectedId));
    li.tabIndex = r.id === selectedId ? 0 : -1;
    const saved = r.response_text ? "saved" : r.query_status;
    li.innerHTML = `
      <div class="item-title">${r.favorite ? `<span class="fav">${icon("star", 12)}</span>` : ""}${escapeHtml(titleOf(r))}</div>
      <div class="item-meta">${escapeHtml(r.destination_name)} · ${when(r.created_at)} · <span class="badge badge-${escapeHtml(saved)}">${escapeHtml(statusLabel(saved))}</span></div>`;
    li.addEventListener("click", () => select(r.id, false));
    listEl.appendChild(li);
  }
}

function select(id: string, focus: boolean) {
  selectedId = id;
  renderList();
  renderDetail();
  if (focus) (listEl.querySelector(".record-item.selected") as HTMLElement | null)?.focus();
}

async function renderDetail() {
  const r = records.find((x) => x.id === selectedId);
  if (!r) {
    detailEl.innerHTML = `
      <div class="empty">
        <h2>${t("rec.emptyTitle")}</h2>
        <p>${t("rec.emptyP1")}</p>
        <p>${t("rec.emptyP2")}</p>
      </div>`;
    return;
  }
  const answer = r.response_markdown || r.response_text;
  const capture = r.capture_status ? (CAPTURE_LABEL[r.capture_status] ? t(CAPTURE_LABEL[r.capture_status]) : r.capture_status) : null;
  detailEl.innerHTML = `
    <header class="detail-head">
      <div class="meta">
        <span>${escapeHtml(r.destination_name)}</span>
        <span>${when(r.created_at)}</span>
        ${r.source_app ? `<span>${escapeHtml(t("rec.from", { app: r.source_app }))}</span>` : ""}
        ${capture ? `<span class="capture capture-${escapeHtml(r.capture_status!)}">${escapeHtml(capture)}</span>` : ""}
      </div>
      <div class="actions">
        <button id="act-fav" class="${r.favorite ? "on" : ""}" aria-pressed="${r.favorite}" title="${t("rec.favorite")}" aria-label="${t("rec.favorite")}">${icon("star", 15)}</button>
        <button id="act-open" title="${t("rec.reopenTitle")}">${t("rec.reopen")}</button>
        <button id="act-copy" title="${t("rec.copyMdTitle")}">${t("rec.copyMd")}</button>
        <button id="act-export" title="${t("rec.exportTitle")}">${t("rec.export")}</button>
        <button id="act-delete" class="danger" title="${t("rec.deleteTitle")}">${t("rec.delete")}</button>
      </div>
    </header>
    <section>
      <h3>${t("rec.original")}</h3>
      <div id="original">${r.selection_text ? `<blockquote>${escapeHtml(r.selection_text)}</blockquote>` : r.attachment_path ? `<p class="muted">${t("rec.loadingShot")}</p>` : `<p class="muted">${t("rec.notRecorded")}</p>`}</div>
    </section>
    ${r.prompt && r.prompt !== r.selection_text ? `<section><h3>${t("rec.prompt")}</h3><pre class="text">${escapeHtml(r.prompt)}</pre></section>` : ""}
    <section>
      <h3>${t("rec.answer")}</h3>
      ${answer ? `<pre class="text answer">${escapeHtml(answer)}</pre>` : `<p class="muted">${t("rec.notSaved")}</p>`}
    </section>
    ${r.citation_links.length ? `<section><h3>${t("rec.links")}</h3><ul class="links">${r.citation_links.map((l) => `<li><a href="${escapeHtml(l)}" target="_blank" rel="noreferrer">${escapeHtml(l)}</a></li>`).join("")}</ul></section>` : ""}
    ${r.conversation_url ? `<p class="muted small">${t("rec.conversation")}<a href="${escapeHtml(r.conversation_url)}" target="_blank" rel="noreferrer">${escapeHtml(r.conversation_url)}</a></p>` : ""}
    <section class="user-fields">
      <label>${t("rec.tags")} <input id="f-tags" type="text" value="${escapeHtml(r.tags.join(", "))}" placeholder="${t("rec.tagsPh")}" /></label>
      <label>${t("rec.note")} <textarea id="f-note" rows="3" placeholder="${t("rec.notePh")}">${escapeHtml(r.note ?? "")}</textarea></label>
    </section>`;

  if (r.attachment_path && !r.selection_text) {
    invoke<string | null>("get_record_attachment", { id: r.id }).then((src) => {
      const box = document.getElementById("original");
      if (box && selectedId === r.id) box.innerHTML = src ? `<img class="shot" src="${src}" alt="${t("rec.shotAlt")}">` : `<p class="muted">${t("rec.shotMissing")}</p>`;
    });
  }

  const saveUser = async (favorite = r.favorite) => {
    const tags = (document.getElementById("f-tags") as HTMLInputElement).value.split(",").map((t) => t.trim()).filter(Boolean);
    const note = (document.getElementById("f-note") as HTMLTextAreaElement).value;
    await invoke("update_record", { id: r.id, tags, note: note || null, favorite });
    r.tags = tags;
    r.note = note || null;
    r.favorite = favorite;
  };
  document.getElementById("f-tags")!.addEventListener("change", () => saveUser());
  document.getElementById("f-note")!.addEventListener("change", () => saveUser());
  document.getElementById("act-fav")!.addEventListener("click", async () => {
    await saveUser(!r.favorite);
    renderList();
    renderDetail();
  });
  document.getElementById("act-open")!.addEventListener("click", () => invoke("open_record", { id: r.id }).catch((e) => alert(e)));
  document.getElementById("act-copy")!.addEventListener("click", async (e) => {
    await invoke("copy_record_markdown", { id: r.id });
    (e.target as HTMLElement).textContent = t("rec.copied");
  });
  document.getElementById("act-export")!.addEventListener("click", () => invoke("export_record_markdown", { id: r.id }).catch((e) => alert(e)));
  document.getElementById("act-delete")!.addEventListener("click", async () => {
    if (!confirm(t("rec.deleteConfirm"))) return;
    await invoke("delete_record", { id: r.id });
    selectedId = null;
    load();
  });
}
