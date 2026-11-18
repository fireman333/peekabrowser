// Plays the lookup highlight; called from Rust via eval (no IPC needed).
const el = document.getElementById("hl")!;

declare global {
  interface Window {
    __pbPlay: (kind: "box" | "dot", pad: number) => void;
  }
}

window.__pbPlay = (kind, pad) => {
  document.documentElement.style.setProperty("--pad", `${pad}px`);
  el.classList.toggle("dot", kind === "dot");
  el.classList.remove("play");
  void el.offsetWidth; // restart the animation
  el.classList.add("play");
};

export {};
