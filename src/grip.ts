// Resize grip: drag to resize the panel from its bottom-right corner. Deltas
// are measured in screen points from where the drag began, so the grip
// window moving under the pointer doesn't matter.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

listen<boolean>("grip-near", (e) => document.body.classList.toggle("near", e.payload));

let start: { x: number; y: number } | null = null;
let pending: { dx: number; dy: number } | null = null;
let frame = 0;

const send = (phase: string, dx: number, dy: number) =>
  invoke("resize_drag", { phase, dx, dy }).catch(() => {});

function flush() {
  frame = 0;
  if (pending) {
    send("move", pending.dx, pending.dy);
    pending = null;
  }
}

document.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  document.body.setPointerCapture(e.pointerId);
  start = { x: e.screenX, y: e.screenY };
  document.body.classList.add("dragging");
  send("start", 0, 0);
});

document.addEventListener("pointermove", (e) => {
  if (!start) return;
  pending = { dx: e.screenX - start.x, dy: e.screenY - start.y };
  if (!frame) frame = requestAnimationFrame(flush);
});

function finish(e: PointerEvent) {
  if (!start) return;
  if (frame) cancelAnimationFrame(frame);
  frame = 0;
  pending = null;
  send("end", e.screenX - start.x, e.screenY - start.y);
  start = null;
  document.body.classList.remove("dragging");
}

document.addEventListener("pointerup", finish);
document.addEventListener("pointercancel", finish);
