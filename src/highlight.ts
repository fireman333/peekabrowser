// "Sent" chip beside the pointer; filled and played from Rust via eval.
import { destIcon, escapeHtml, installFaviconFallback } from "./icons";

const chip = document.getElementById("chip")!;
installFaviconFallback(chip);

interface Sent {
  name: string;
  icon: string;
  url: string;
  snippet: string;
  alignRight: boolean;
}

declare global {
  interface Window {
    __pbSent: (s: Sent) => void;
  }
}

window.__pbSent = (s) => {
  chip.innerHTML = `
    <span class="chip-icon">${destIcon({ icon: s.icon, url: s.url }, 22)}</span>
    <span class="chip-text">
      <span class="chip-name">${escapeHtml(s.name)}</span>
      <span class="chip-snippet">${escapeHtml(s.snippet)}</span>
    </span>
    <span class="chip-check" aria-hidden="true">✓</span>`;
  document.body.classList.toggle("right", s.alignRight);
  chip.classList.remove("play");
  void chip.offsetWidth; // restart the animation
  chip.classList.add("play");
};

export {};
