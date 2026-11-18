// Settings → prompts: the profession template list, one template's prompts,
// and the per-destination prompt list on a site's page. A port of the iOS
// PromptTemplateListView / PromptTemplatePromptsView / DestinationPromptsSection.
// Every change is saved right away (the backend owns prompts.json).
import { invoke } from "@tauri-apps/api/core";
import { escapeHtml } from "./icons";
import { onLangChange, t, type Key } from "./i18n";

export interface Lens {
  id: string;
  title: string;
  prompt: string;
}

interface TemplateInfo {
  id: string;
  edited: boolean;
  prompts: Lens[];
}

interface Templates {
  profession: string;
  templates: TemplateInfo[];
}

const PROFESSIONS = ["general", "student", "physician", "lawyer", "researcher", "engineer", "teacher"];

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const profTitle = (p: string) => t(`prof.${p}` as Key);

/** The prompt form's save rule (mirrors `with_trailing_space` in prompts.rs). */
export function withTrailingSpace(s: string): string {
  return s && !/\s$/.test(s) ? s + " " : s;
}

// ─── Browser-only preview (no Tauri backend) ───────────────

const isPreview = () => !(window as any).__TAURI_INTERNALS__;

function previewTemplates(): Templates {
  const sample = (p: string): Lens[] => [
    { id: `${p}.a`, title: `${profTitle(p)} 1`, prompt: "Explain this simply: " },
    { id: `${p}.b`, title: `${profTitle(p)} 2`, prompt: "Summarize the key points of: " },
  ];
  return { profession: "general", templates: PROFESSIONS.map((id) => ({ id, edited: false, prompts: sample(id) })) };
}

let previewData: Templates | null = null;
const previewSites = new Map<string, Lens[]>();

async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!isPreview()) return invoke<T>(cmd, args);
  previewData ??= previewTemplates();
  const assignIds = (l: Lens[]) => l.map((x) => (x.id ? x : { ...x, id: crypto.randomUUID() }));
  switch (cmd) {
    case "get_prompt_templates": return previewData as T;
    case "set_prompt_profession": previewData.profession = args.profession as string; return previewData as T;
    case "set_template_prompts":
      previewData.templates.find((x) => x.id === args.profession)!.prompts = assignIds(args.prompts as Lens[]);
      return previewData as T;
    case "restore_template_prompts": return previewData as T;
    case "get_destination_prompts":
      return (previewSites.get(args.destinationId as string) ?? previewData.templates[0].prompts) as T;
    case "set_destination_prompts":
      previewSites.set(args.destinationId as string, assignIds(args.prompts as Lens[]));
      return previewSites.get(args.destinationId as string) as T;
  }
  throw new Error(`no preview for ${cmd}`);
}

// ─── Prompt form ───────────────────────────────────────────

interface FormHandlers {
  save: (l: Lens) => void;
  remove?: () => void;
}

let formHandlers: FormHandlers | null = null;
let formLens: Lens | null = null;

function setupForm() {
  const dialog = $<HTMLDialogElement>("prompt-dialog");
  const name = $<HTMLInputElement>("prompt-name");
  const text = $<HTMLTextAreaElement>("prompt-text");
  const save = $<HTMLButtonElement>("prompt-save");
  const validate = () => { save.disabled = !name.value.trim() || !text.value.trim(); };
  name.addEventListener("input", validate);
  text.addEventListener("input", validate);
  $("prompt-cancel").addEventListener("click", () => dialog.close());
  $("prompt-delete").addEventListener("click", () => {
    const h = formHandlers;
    if (!h?.remove || !formLens) return;
    if (!confirm(t("p.deleteConfirm", { name: formLens.title }))) return;
    dialog.close();
    h.remove();
  });
  $("prompt-form").addEventListener("submit", (e) => {
    e.preventDefault();
    validate();
    if (save.disabled || !formHandlers) return;
    const lens: Lens = { id: formLens?.id ?? "", title: name.value.trim(), prompt: withTrailingSpace(text.value) };
    dialog.close();
    formHandlers.save(lens);
  });
  dialog.addEventListener("close", () => { formHandlers = null; formLens = null; });
}

function openForm(lens: Lens | null, handlers: FormHandlers) {
  formLens = lens;
  formHandlers = handlers;
  $<HTMLInputElement>("prompt-name").value = lens?.title ?? "";
  $<HTMLTextAreaElement>("prompt-text").value = lens?.prompt ?? "";
  $<HTMLButtonElement>("prompt-save").disabled = !lens;
  $("prompt-delete").classList.toggle("hidden", !handlers.remove);
  $<HTMLDialogElement>("prompt-dialog").showModal();
  $<HTMLInputElement>("prompt-name").focus();
}

// ─── A reorderable prompt list (template or destination) ──

class PromptListEditor {
  list: Lens[] = [];
  private reordering = false;
  private dragFrom = -1;
  private readonly listEl: HTMLElement;
  private readonly toggle: HTMLButtonElement;

  constructor(section: HTMLElement, private readonly commit: (l: Lens[]) => Promise<Lens[]>) {
    this.listEl = section.querySelector<HTMLElement>(".prompt-list")!;
    this.toggle = section.querySelector<HTMLButtonElement>(".prompts-reorder")!;
    this.toggle.addEventListener("click", () => { this.reordering = !this.reordering; this.render(); });
    section.querySelector(".prompts-add")!.addEventListener("click", () => {
      openForm(null, { save: (l) => this.apply([...this.list, { ...l, id: "" }]) });
    });
  }

  /** Show a list; `keepMode` keeps Reorder on (a refresh after this editor's own change). */
  set(list: Lens[], keepMode = false) {
    this.list = list;
    if (!keepMode) this.reordering = false;
    this.render();
  }

  private async apply(next: Lens[]) {
    try {
      this.list = await this.commit(next);
    } catch (e) {
      alert(t("p.saveFailed") + e);
    }
    this.render();
  }

  private move(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || to >= this.list.length) return;
    const next = [...this.list];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    this.apply(next);
  }

  render() {
    this.toggle.textContent = t(this.reordering ? "s.done" : "s.reorder");
    this.toggle.classList.toggle("hidden", this.list.length < 2 && !this.reordering);
    this.listEl.classList.toggle("reordering", this.reordering);
    this.listEl.innerHTML = "";
    if (this.list.length === 0) {
      this.listEl.innerHTML = `<div class="dest-empty">${t("p.empty")}</div>`;
      return;
    }
    this.list.forEach((l, i) => {
      const el = document.createElement("div");
      el.className = "prompt-item";
      el.draggable = this.reordering;
      el.tabIndex = this.reordering ? -1 : 0;
      el.setAttribute("role", "button");
      el.innerHTML = `
        ${this.reordering ? `<span class="grip" aria-hidden="true">≡</span>` : ""}
        <div class="info">
          <div class="name">${escapeHtml(l.title)}</div>
          <div class="sub">${escapeHtml(l.prompt)}</div>
        </div>
        <div class="reorder-btns">
          <button class="icon-btn up-btn" title="${t("dest.moveUp")}" aria-label="${t("dest.moveUp")}" ${i === 0 ? "disabled" : ""}>↑</button>
          <button class="icon-btn down-btn" title="${t("dest.moveDown")}" aria-label="${t("dest.moveDown")}" ${i === this.list.length - 1 ? "disabled" : ""}>↓</button>
        </div>
        <span class="chev" aria-hidden="true">›</span>`;
      const edit = () => {
        if (this.reordering) return;
        openForm(l, {
          save: (changed) => this.apply(this.list.map((x, j) => (j === i ? changed : x))),
          remove: () => this.apply(this.list.filter((_, j) => j !== i)),
        });
      };
      el.addEventListener("click", edit);
      el.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); edit(); } });
      el.querySelector(".up-btn")!.addEventListener("click", (e) => { e.stopPropagation(); this.move(i, i - 1); });
      el.querySelector(".down-btn")!.addEventListener("click", (e) => { e.stopPropagation(); this.move(i, i + 1); });
      el.addEventListener("dragstart", (e) => { this.dragFrom = i; e.dataTransfer?.setData("text/plain", String(i)); el.classList.add("dragging"); });
      el.addEventListener("dragend", () => el.classList.remove("dragging"));
      el.addEventListener("dragover", (e) => { if (this.reordering) { e.preventDefault(); el.classList.add("drop-target"); } });
      el.addEventListener("dragleave", () => el.classList.remove("drop-target"));
      el.addEventListener("drop", (e) => {
        e.preventDefault();
        el.classList.remove("drop-target");
        this.move(this.dragFrom, i);
      });
      this.listEl.appendChild(el);
    });
  }
}

// ─── Views ─────────────────────────────────────────────────

let data: Templates | null = null;
let openTemplate: string | null = null;
let siteId: string | null = null;
let templateEditor: PromptListEditor;
let siteEditor: PromptListEditor;

const VIEWS = ["view-main", "view-site", "view-templates", "view-template"];

function show(view: string) {
  VIEWS.forEach((v) => $(v).classList.toggle("hidden", v !== view));
  window.scrollTo(0, 0);
}

async function loadTemplates() {
  try {
    data = await call<Templates>("get_prompt_templates");
  } catch (e) {
    console.warn("prompts: load failed", e);
  }
  renderAll();
}

function renderAll(keepMode = false) {
  $("prompts-profession").textContent = data ? profTitle(data.profession) : "";
  renderTemplateList();
  if (openTemplate) renderTemplate(keepMode);
}

function renderTemplateList() {
  const list = $("template-list");
  list.innerHTML = "";
  if (!data) return;
  for (const tpl of data.templates) {
    const selected = tpl.id === data.profession;
    const row = document.createElement("div");
    row.className = "row template-row";
    row.innerHTML = `
      <button type="button" class="choose" role="radio" aria-checked="${selected}"
        aria-label="${escapeHtml(profTitle(tpl.id))}" title="${t("p.use")}"></button>
      <button type="button" class="open">
        <span class="row-text">
          <strong>${escapeHtml(profTitle(tpl.id))}</strong>
          <small>${t("p.count", { n: tpl.prompts.length })}${selected ? ` · <span class="selected-tag">${t("p.selected")}</span>` : ""}</small>
        </span>
        <span class="chev" aria-hidden="true">›</span>
      </button>`;
    row.querySelector(".choose")!.addEventListener("click", async () => {
      if (!data || tpl.id === data.profession) return;
      try {
        data = await call<Templates>("set_prompt_profession", { profession: tpl.id });
      } catch (e) {
        alert(t("p.saveFailed") + e);
      }
      renderAll();
    });
    row.querySelector(".open")!.addEventListener("click", () => {
      openTemplate = tpl.id;
      renderTemplate();
      show("view-template");
    });
    list.appendChild(row);
  }
}

function renderTemplate(keepMode = false) {
  const tpl = data?.templates.find((x) => x.id === openTemplate);
  if (!tpl) return;
  $("template-title").textContent = profTitle(tpl.id);
  $("to-templates").textContent = `‹ ${t("p.row")}`;
  templateEditor.set(tpl.prompts, keepMode);
}

/** Show the site page's prompt list (null id = a site not saved yet). */
export async function loadSitePrompts(destinationId: string | null, url: string) {
  const system = url.startsWith("system://") || url.startsWith("https://system://");
  siteId = system ? null : destinationId;
  $("site-prompts-section").classList.toggle("hidden", !destinationId || system);
  $("site-prompts-later").classList.toggle("hidden", !!destinationId || system);
  if (!destinationId || system) return;
  try {
    siteEditor.set(await call<Lens[]>("get_destination_prompts", { destinationId }));
  } catch (e) {
    console.warn("prompts: site load failed", e);
    siteEditor.set([]);
  }
}

export function setupPrompts(showMain: () => void) {
  setupForm();
  templateEditor = new PromptListEditor($("template-prompts-section"), async (prompts) => {
    data = await call<Templates>("set_template_prompts", { profession: openTemplate, prompts });
    renderAll(true);
    return data.templates.find((x) => x.id === openTemplate)?.prompts ?? [];
  });
  siteEditor = new PromptListEditor($("site-prompts-section"), (prompts) =>
    call<Lens[]>("set_destination_prompts", { destinationId: siteId, prompts }),
  );

  $("open-prompts").addEventListener("click", () => { renderTemplateList(); show("view-templates"); });
  document.querySelectorAll(".to-main").forEach((b) => b.addEventListener("click", showMain));
  $("to-templates").addEventListener("click", () => { openTemplate = null; renderTemplateList(); show("view-templates"); });
  $("restore-template").addEventListener("click", async () => {
    if (!openTemplate || !confirm(t("p.restoreConfirm"))) return;
    try {
      data = await call<Templates>("restore_template_prompts", { profession: openTemplate });
    } catch (e) {
      alert(t("p.saveFailed") + e);
    }
    renderAll();
  });

  // Built-in text follows the interface language until edited: reload.
  onLangChange(() => {
    loadTemplates(); // re-renders the open template too
    if (siteId) loadSitePrompts(siteId, "");
    templateEditor.render();
  });
  return loadTemplates();
}
