// Input-method (注音, Pinyin, Kana …) guard for Enter handlers.
//
// WebKit sends the Enter that confirms a composition as a normal keydown
// with `key === "Enter"`, `isComposing === false` and `keyCode === 229`, and
// fires `compositionend` before that keydown. So check both the event and a
// composition flag that is cleared one task after `compositionend`.

/** True if this key event belongs to an IME composition. */
export function isImeKey(e: KeyboardEvent): boolean {
  return e.isComposing || e.keyCode === 229;
}

/** Track composition on `el`; the returned function says whether one is in
 *  progress (or just ended, for the Enter that confirmed it). */
export function trackComposition(el: HTMLElement): () => boolean {
  let composing = false;
  el.addEventListener("compositionstart", () => {
    composing = true;
  });
  el.addEventListener("compositionend", () => {
    setTimeout(() => {
      composing = false;
    }, 0);
  });
  return () => composing;
}
