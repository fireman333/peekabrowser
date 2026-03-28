// Airy Text Injector - injected into all destination webviews
// Provides window.__airyInjectText(text) function

window.__airyInjectText = function(text) {
  var destId = window.__airyDestId || '';
  var url = window.location.hostname;

  if (url.includes('google.com')) {
    injectGoogle(text);
  } else if (url.includes('openai.com') || url.includes('chatgpt.com')) {
    injectChatGPT(text);
  } else if (url.includes('claude.ai')) {
    injectClaude(text);
  } else if (url.includes('perplexity.ai')) {
    injectPerplexity(text);
  } else {
    injectGeneric(text);
  }
};

function setNativeValue(element, value) {
  var nativeInputValueSetter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype, 'value'
  ) || Object.getOwnPropertyDescriptor(
    window.HTMLTextAreaElement.prototype, 'value'
  );
  if (nativeInputValueSetter && nativeInputValueSetter.set) {
    nativeInputValueSetter.set.call(element, value);
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }
  return false;
}

function injectContentEditable(el, text) {
  el.focus();
  // Select all existing content and replace
  document.execCommand('selectAll', false, null);
  document.execCommand('insertText', false, text);
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

function injectGoogle(text) {
  var selectors = ['textarea[name="q"]', 'input[name="q"]', 'input[type="text"]'];
  for (var i = 0; i < selectors.length; i++) {
    var el = document.querySelector(selectors[i]);
    if (el) {
      setNativeValue(el, text);
      el.form && el.form.submit();
      return;
    }
  }
  injectGeneric(text);
}

function injectChatGPT(text) {
  var selectors = [
    '#prompt-textarea',
    'textarea[data-id="prompt-textarea"]',
    'textarea',
    'div.ProseMirror[contenteditable="true"]',
    '[contenteditable="true"]'
  ];
  for (var i = 0; i < selectors.length; i++) {
    var el = document.querySelector(selectors[i]);
    if (el) {
      if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {
        setNativeValue(el, text);
      } else {
        injectContentEditable(el, text);
      }
      return;
    }
  }
}

function injectClaude(text) {
  var selectors = [
    'div.ProseMirror[contenteditable="true"]',
    'div[contenteditable="true"]',
    'textarea',
    '[contenteditable="true"]'
  ];
  for (var i = 0; i < selectors.length; i++) {
    var el = document.querySelector(selectors[i]);
    if (el) {
      if (el.tagName === 'TEXTAREA') {
        setNativeValue(el, text);
      } else {
        injectContentEditable(el, text);
      }
      return;
    }
  }
}

function injectPerplexity(text) {
  var selectors = [
    'textarea',
    'div.ProseMirror[contenteditable="true"]',
    '[contenteditable="true"]',
    'input[type="text"]'
  ];
  for (var i = 0; i < selectors.length; i++) {
    var el = document.querySelector(selectors[i]);
    if (el) {
      if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {
        setNativeValue(el, text);
      } else {
        injectContentEditable(el, text);
      }
      return;
    }
  }
}

function injectGeneric(text) {
  var selectors = ['textarea', 'input[type="text"]', '[contenteditable="true"]'];
  for (var i = 0; i < selectors.length; i++) {
    var el = document.querySelector(selectors[i]);
    if (el) {
      if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {
        setNativeValue(el, text);
      } else {
        injectContentEditable(el, text);
      }
      return;
    }
  }
}
