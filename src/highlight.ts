// Plays the lookup highlight; called from Rust via eval (no IPC needed).
const el = document.getElementById("hl")!;

declare global {
  interface Window {
    __pbPlay: (kind: "box" | "pill", pad: number) => void;
  }
}

window.__pbPlay = (kind, pad) => {
  document.documentElement.style.setProperty("--pad", `${pad}px`);
  el.classList.toggle("pill", kind === "pill");
  el.classList.remove("play");
  void el.offsetWidth; // restart the animation
  el.classList.add("play");
};

export {};
