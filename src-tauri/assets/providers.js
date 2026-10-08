// Peekabrowser provider adapters.
//
// Evaluated on demand (never as a resident script): Rust prepends this file to
// a call such as `return P.inject({...})` and reads back a JSON string. Nothing
// here installs timers or observers. Per-page state lives on a non-enumerable
// symbol so it doesn't show up as a page global.
(function () {
  var KEY = Symbol.for('peekabrowser.state');
  var state = window[KEY];
  if (!state) {
    state = { done: {}, baseline: {} };
    try { Object.defineProperty(window, KEY, { value: state, enumerable: false, configurable: true }); }
    catch (e) { window[KEY] = state; }
  }

  function visible(el) {
    if (!el) return false;
    if (el.offsetParent !== null) return true;
    var r = el.getBoundingClientRect ? el.getBoundingClientRect() : null;
    return !!(r && r.width > 0 && r.height > 0);
  }
  function first(selectors) {
    for (var i = 0; i < selectors.length; i++) {
      var list = document.querySelectorAll(selectors[i]);
      for (var j = 0; j < list.length; j++) if (visible(list[j])) return list[j];
    }
    return null;
  }
  function last(selectors) {
    for (var i = 0; i < selectors.length; i++) {
      var list = document.querySelectorAll(selectors[i]);
      if (list.length) return list[list.length - 1];
    }
    return null;
  }

  // ─── Provider table ──────────────────────────────────────────────
  // supportsText / supportsImage, input / send / stop (generating) / response selectors.
  var PROVIDERS = [
    {
      id: 'google',
      match: function (h) { return /(^|\.)google\.[a-z.]+$/.test(h) && h.indexOf('gemini') < 0 && h.indexOf('accounts') < 0; },
      supportsText: true, supportsImage: false, submitForm: true,
      input: ['textarea[name="q"]', 'input[name="q"]'],
      send: [], stop: [], response: [], knowsCompletion: false
    },
    {
      id: 'chatgpt',
      match: function (h) { return h === 'chatgpt.com' || h.endsWith('.chatgpt.com') || h === 'chat.openai.com'; },
      supportsText: true, supportsImage: true,
      input: ['#prompt-textarea', 'div.ProseMirror[contenteditable="true"]', 'textarea'],
      send: ['button[data-testid="send-button"]', 'button[aria-label*="Send"]'],
      stop: ['button[data-testid="stop-button"]', 'button[aria-label*="Stop"]'],
      response: ['[data-message-author-role="assistant"] .markdown', '[data-message-author-role="assistant"]'],
      knowsCompletion: true
    },
    {
      id: 'claude',
      match: function (h) { return h === 'claude.ai' || h.endsWith('.claude.ai'); },
      supportsText: true, supportsImage: true,
      input: ['div.ProseMirror[contenteditable="true"]', '[contenteditable="true"]'],
      send: ['button[aria-label*="Send"]', 'button[aria-label*="送出"]'],
      stop: ['button[aria-label*="Stop"]', '[data-is-streaming="true"]'],
      response: ['.font-claude-response', '[data-is-streaming] .font-claude-message', '.font-claude-message', '[data-is-streaming]'],
      knowsCompletion: true
    },
    {
      id: 'gemini',
      match: function (h) { return h === 'gemini.google.com'; },
      supportsText: true, supportsImage: true,
      input: ['rich-textarea .ql-editor', 'rich-textarea [contenteditable="true"]', '[contenteditable="true"]'],
      send: ['button.send-button', 'button[aria-label*="Send"]', 'button[aria-label*="傳送"]', 'button[aria-label*="送出"]'],
      stop: ['button[aria-label*="Stop"]', 'button[aria-label*="停止"]'],
      response: ['model-response message-content', 'message-content', 'model-response'],
      knowsCompletion: true
    },
    {
      id: 'perplexity',
      match: function (h) { return h === 'perplexity.ai' || h.endsWith('.perplexity.ai'); },
      supportsText: true, supportsImage: true,
      input: ['#ask-input', 'textarea', 'div[contenteditable="true"]'],
      send: ['button[aria-label="Submit"]', 'button[aria-label*="Submit"]', 'button[data-testid="submit-button"]'],
      stop: ['button[aria-label*="Stop"]'],
      response: ['.prose'],
      knowsCompletion: true
    },
    {
      id: 'openevidence',
      match: function (h) { return h === 'openevidence.com' || h.endsWith('.openevidence.com'); },
      supportsText: true, supportsImage: false,
      input: ['textarea', 'input[type="text"]', '[contenteditable="true"]'],
      send: ['button[type="submit"]', 'button[aria-label*="Submit"]'],
      stop: ['button[aria-label*="Stop"]'],
      response: ['.prose', 'article'],
      knowsCompletion: false
    }
  ];
  var GENERIC = {
    id: 'generic', supportsText: true, supportsImage: true,
    input: ['textarea[placeholder*="Message"]', 'textarea[placeholder*="Ask"]', 'textarea:not([readonly])',
      'input[type="search"]:not([readonly])', 'input[type="text"]:not([readonly])', '[contenteditable="true"]'],
    send: ['button[aria-label*="Send"]', 'button[aria-label*="send"]', 'button[type="submit"]'],
    stop: [], response: [], knowsCompletion: false
  };

  function provider() {
    var h = '';
    try { h = location.hostname; } catch (e) {}
    for (var i = 0; i < PROVIDERS.length; i++) if (PROVIDERS[i].match(h)) return PROVIDERS[i];
    return GENERIC;
  }

  // ─── Input helpers ───────────────────────────────────────────────
  function setText(el, text) {
    el.focus();
    if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {
      var proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      var desc = Object.getOwnPropertyDescriptor(proto, 'value');
      if (desc && desc.set) desc.set.call(el, text); else el.value = text;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    } else {
      document.execCommand('selectAll', false, null);
      if (!document.execCommand('insertText', false, text)) {
        el.textContent = text;
        el.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
      }
    }
  }
  function inputHasText(el) {
    if (!el) return false;
    var v = (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') ? el.value : el.textContent;
    return !!(v && v.trim().length);
  }
  function pressEnter(el) {
    ['keydown', 'keypress', 'keyup'].forEach(function (t) {
      el.dispatchEvent(new KeyboardEvent(t, { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }));
    });
  }
  function submit(p, el) {
    if (p.submitForm) {
      var form = el.closest('form');
      if (form) { form.submit(); return; }
    }
    var tries = 0;
    (function attempt() {
      var btn = first(p.send);
      if (btn && !btn.disabled && btn.getAttribute('aria-disabled') !== 'true') { btn.click(); return; }
      if (++tries < 8) { setTimeout(attempt, 250); return; }
      pressEnter(el);
    })();
  }
  function attachmentCount() {
    return document.querySelectorAll(
      'form img, [data-testid*="attachment"], [class*="attachment"], [class*="file-preview"], ' +
      '[class*="FilePreview"], [aria-label*="Remove file"], [aria-label*="移除"], uploader-file-preview, .file-preview'
    ).length;
  }
  function dataUrlToFile(dataUrl) {
    var parts = dataUrl.split(',');
    var mime = (parts[0].match(/:(.*?);/) || [])[1] || 'image/png';
    var bin = atob(parts[1]);
    var arr = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
    return new File([arr], 'screenshot.png', { type: mime });
  }

  // ─── Status ──────────────────────────────────────────────────────
  function generating(p) {
    for (var i = 0; i < p.stop.length; i++) {
      var list = document.querySelectorAll(p.stop[i]);
      for (var j = 0; j < list.length; j++) {
        var el = list[j];
        if (el.hasAttribute('data-is-streaming') || visible(el)) return true;
      }
    }
    return false;
  }

  // ─── HTML → Markdown ─────────────────────────────────────────────
  var SKIP = { SCRIPT: 1, STYLE: 1, BUTTON: 1, SVG: 1, NOSCRIPT: 1, TEMPLATE: 1, INPUT: 1, TEXTAREA: 1, SELECT: 1 };
  function md(node, ctx) {
    if (node.nodeType === 3) return ctx.pre ? node.nodeValue : node.nodeValue.replace(/\s+/g, ' ');
    if (node.nodeType !== 1) return '';
    var tag = node.tagName.toUpperCase();
    if (SKIP[tag] || node.getAttribute('aria-hidden') === 'true' || node.hasAttribute('data-peeka-ignore')) return '';
    // KaTeX / MathJax: keep the TeX source.
    if (node.classList && (node.classList.contains('katex') || node.classList.contains('katex-display'))) {
      var ann = node.querySelector('annotation[encoding="application/x-tex"]');
      if (ann) {
        var display = node.classList.contains('katex-display') || !!node.closest('.katex-display');
        return display ? '\n\n$$' + ann.textContent + '$$\n\n' : '$' + ann.textContent + '$';
      }
    }
    if (tag === 'MATH' && node.getAttribute('alttext')) return '$' + node.getAttribute('alttext') + '$';
    var kids = function (c) {
      var s = '';
      for (var i = 0; i < node.childNodes.length; i++) s += md(node.childNodes[i], c || ctx);
      return s;
    };
    switch (tag) {
      case 'H1': case 'H2': case 'H3': case 'H4': case 'H5': case 'H6':
        return '\n\n' + '######'.slice(0, +tag[1]) + ' ' + kids().trim() + '\n\n';
      case 'P': case 'DIV': case 'SECTION': case 'ARTICLE':
        var inner = kids();
        return tag === 'P' ? '\n\n' + inner.trim() + '\n\n' : inner + (/\n$/.test(inner) ? '' : '\n');
      case 'BR': return '  \n';
      case 'HR': return '\n\n---\n\n';
      case 'STRONG': case 'B': var b = kids().trim(); return b ? '**' + b + '**' : '';
      case 'EM': case 'I': var it = kids().trim(); return it ? '*' + it + '*' : '';
      case 'DEL': case 'S': return '~~' + kids().trim() + '~~';
      case 'CODE':
        if (ctx.pre) return node.textContent;
        return '`' + node.textContent.replace(/`/g, '\\`') + '`';
      case 'PRE':
        var codeEl = node.querySelector('code') || node;
        var lang = '';
        var cls = (codeEl.className || '') + ' ' + (node.className || '');
        var m = cls.match(/language-([\w+#-]+)/);
        if (m) lang = m[1];
        return '\n\n```' + lang + '\n' + codeEl.textContent.replace(/\n$/, '') + '\n```\n\n';
      case 'A':
        var href = node.getAttribute('href') || '';
        var label = kids().trim();
        if (!href || href.indexOf('javascript:') === 0) return label;
        try { href = new URL(href, location.href).href; } catch (e) {}
        ctx.links.push(href);
        return '[' + (label || href) + '](' + href + ')';
      case 'IMG':
        var alt = node.getAttribute('alt') || '';
        var src = node.getAttribute('src') || '';
        return src && src.indexOf('data:') !== 0 ? '![' + alt + '](' + src + ')' : (alt ? '[' + alt + ']' : '');
      case 'BLOCKQUOTE':
        return '\n\n' + kids().trim().split('\n').map(function (l) { return '> ' + l; }).join('\n') + '\n\n';
      case 'UL': case 'OL':
        var out = '\n';
        var n = 1;
        for (var i = 0; i < node.children.length; i++) {
          var li = node.children[i];
          if (li.tagName !== 'LI') continue;
          var bullet = tag === 'OL' ? (n++) + '. ' : '- ';
          var body = md(li, { pre: false, depth: ctx.depth + 1, links: ctx.links }).trim()
            .replace(/\n{2,}/g, '\n').replace(/\n/g, '\n' + '   ');
          out += '  '.repeat(ctx.depth) + bullet + body + '\n';
        }
        return out + '\n';
      case 'LI': return kids();
      case 'TABLE':
        var rows = node.querySelectorAll('tr');
        if (!rows.length) return '';
        var lines = [];
        for (var r = 0; r < rows.length; r++) {
          var cells = rows[r].querySelectorAll('th, td');
          var vals = [];
          for (var c = 0; c < cells.length; c++) {
            vals.push(md(cells[c], { pre: false, depth: 0, links: ctx.links }).trim().replace(/\n+/g, ' ').replace(/\|/g, '\\|'));
          }
          lines.push('| ' + vals.join(' | ') + ' |');
          if (r === 0) lines.push('|' + vals.map(function () { return ' --- '; }).join('|') + '|');
        }
        return '\n\n' + lines.join('\n') + '\n\n';
      default:
        if (tag === 'PRE' || ctx.pre) return kids({ pre: true, depth: ctx.depth, links: ctx.links });
        return kids();
    }
  }
  function toMarkdown(el) {
    var ctx = { pre: false, depth: 0, links: [] };
    var out = md(el, ctx).replace(/[ \t]+\n/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
    var seen = {};
    var links = ctx.links.filter(function (l) { if (seen[l]) return false; seen[l] = 1; return true; });
    return { markdown: out, links: links };
  }

  var P = {
    status: function () {
      var p = provider();
      var input = first(p.input);
      return {
        provider: p.id, url: location.href, title: document.title || '',
        ready: document.readyState !== 'loading', hasInput: !!input,
        generating: generating(p), draft: inputHasText(input), knowsCompletion: !!p.knowsCompletion
      };
    },

    // payload: { requestId, kind: 'text'|'image', text, submit }
    inject: function (payload) {
      var p = provider();
      if (state.done[payload.requestId]) return { status: 'duplicate', provider: p.id };
      if (document.readyState === 'loading') return { status: 'not_ready', provider: p.id };
      var el = first(p.input);
      if (!el) return { status: 'no_input', provider: p.id };
      if (payload.kind === 'image') {
        if (!p.supportsImage) return { status: 'unsupported', provider: p.id };
        state.baseline[payload.requestId] = attachmentCount();
        var file = dataUrlToFile(payload.imageDataUrl);
        el.focus();
        var dt = new DataTransfer();
        dt.items.add(file);
        el.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
        state.done[payload.requestId] = true;
        return { status: 'pasted', provider: p.id };
      }
      setText(el, payload.text);
      state.done[payload.requestId] = true;
      if (payload.submit) submit(p, el);
      return { status: 'ok', provider: p.id };
    },

    imageAccepted: function (requestId) {
      var base = state.baseline[requestId];
      return { accepted: base !== undefined && attachmentCount() > base };
    },

    // Fallback only when paste was not accepted.
    dropImage: function (payload) {
      var p = provider();
      var el = first(p.input);
      if (!el) return { status: 'no_input' };
      var file = dataUrlToFile(payload.imageDataUrl);
      var dt = new DataTransfer();
      dt.items.add(file);
      ['dragenter', 'dragover', 'drop'].forEach(function (t) {
        el.dispatchEvent(new DragEvent(t, { dataTransfer: dt, bubbles: true, cancelable: true }));
      });
      return { status: 'dropped' };
    },

    // Put the prompt next to an attached image without submitting (upload may still be running).
    insertPrompt: function (text) {
      var p = provider();
      var el = first(p.input);
      if (!el || !text) return { status: el ? 'ok' : 'no_input' };
      el.focus();
      if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') setText(el, text);
      else document.execCommand('insertText', false, text);
      return { status: 'ok' };
    },

    // Save answer: the user's selection wins; otherwise the provider's last response.
    extract: function () {
      var p = provider();
      var sel = window.getSelection ? window.getSelection() : null;
      if (sel && sel.rangeCount && String(sel).trim().length) {
        var box = document.createElement('div');
        for (var i = 0; i < sel.rangeCount; i++) box.appendChild(sel.getRangeAt(i).cloneContents());
        var r = toMarkdown(box);
        return { text: String(sel), markdown: r.markdown, links: r.links, conversation_url: location.href,
          capture_status: 'manual_selection', provider: p.id };
      }
      var el = last(p.response);
      if (!el) return { text: '', markdown: '', links: [], conversation_url: location.href, capture_status: 'none', provider: p.id };
      var res = toMarkdown(el);
      var status = p.knowsCompletion ? (generating(p) ? 'partial' : 'complete') : 'unknown';
      return { text: el.innerText || el.textContent || '', markdown: res.markdown, links: res.links,
        conversation_url: location.href, capture_status: status, provider: p.id };
    }
  };
  return P;
})()
