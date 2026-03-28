// Airy Response Extractor - injected into all destination webviews
// Monitors for AI responses and adds copy toolbar

window.__airyGetLastResponse = function() {
  var url = window.location.hostname;
  if (url.includes('openai.com') || url.includes('chatgpt.com')) {
    return getLastChatGPTResponse();
  } else if (url.includes('claude.ai')) {
    return getLastClaudeResponse();
  } else if (url.includes('perplexity.ai')) {
    return getLastPerplexityResponse();
  }
  return '';
};

function getLastChatGPTResponse() {
  var blocks = document.querySelectorAll('[data-message-author-role="assistant"]');
  if (blocks.length === 0) return '';
  var last = blocks[blocks.length - 1];
  return last ? last.innerText || last.textContent || '' : '';
}

function getLastClaudeResponse() {
  var blocks = document.querySelectorAll('[data-is-streaming="false"]');
  if (blocks.length === 0) {
    // Try alternate selectors
    blocks = document.querySelectorAll('.font-claude-message');
  }
  if (blocks.length === 0) return '';
  var last = blocks[blocks.length - 1];
  return last ? last.innerText || last.textContent || '' : '';
}

function getLastPerplexityResponse() {
  var blocks = document.querySelectorAll('.prose');
  if (blocks.length === 0) return '';
  var last = blocks[blocks.length - 1];
  return last ? last.innerText || last.textContent || '' : '';
}

// Watch for new response blocks and add toolbar
var airyObserver = new MutationObserver(function(mutations) {
  for (var i = 0; i < mutations.length; i++) {
    var mut = mutations[i];
    for (var j = 0; j < mut.addedNodes.length; j++) {
      var node = mut.addedNodes[j];
      if (node.nodeType === 1) {
        checkAndAddToolbar(node);
      }
    }
  }
});

airyObserver.observe(document.body, { childList: true, subtree: true });

function checkAndAddToolbar(node) {
  // Only add toolbar to completed responses (not streaming)
  var url = window.location.hostname;
  var isResponse = false;

  if (url.includes('openai.com') || url.includes('chatgpt.com')) {
    isResponse = node.matches && node.matches('[data-message-author-role="assistant"]');
  } else if (url.includes('claude.ai')) {
    isResponse = node.matches && node.matches('[data-is-streaming="false"]');
  }

  if (isResponse && !node.querySelector('.airy-toolbar')) {
    addCopyToolbar(node);
  }
}

function addCopyToolbar(responseEl) {
  var toolbar = document.createElement('div');
  toolbar.className = 'airy-toolbar';
  toolbar.style.cssText = [
    'display: flex',
    'gap: 6px',
    'margin-top: 8px',
    'padding: 4px',
  ].join(';');

  var copyBtn = createToolbarButton('Copy Text', function() {
    var text = responseEl.innerText || responseEl.textContent || '';
    navigator.clipboard.writeText(text).catch(function() {
      // Fallback
      var ta = document.createElement('textarea');
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
    });
    copyBtn.textContent = 'Copied!';
    setTimeout(function() { copyBtn.textContent = 'Copy Text'; }, 1500);
  });

  var copyMdBtn = createToolbarButton('Copy Markdown', function() {
    // Try to get markdown version via innerText (approximate)
    var text = responseEl.innerText || responseEl.textContent || '';
    navigator.clipboard.writeText(text).catch(function() {});
    copyMdBtn.textContent = 'Copied!';
    setTimeout(function() { copyMdBtn.textContent = 'Copy Markdown'; }, 1500);
  });

  toolbar.appendChild(copyBtn);
  toolbar.appendChild(copyMdBtn);

  // Make response draggable
  responseEl.setAttribute('draggable', 'true');
  responseEl.addEventListener('dragstart', function(e) {
    var text = responseEl.innerText || responseEl.textContent || '';
    e.dataTransfer.setData('text/plain', text);
    e.dataTransfer.setData('text/html', responseEl.innerHTML);
  });

  responseEl.appendChild(toolbar);
}

function createToolbarButton(label, onClick) {
  var btn = document.createElement('button');
  btn.textContent = label;
  btn.style.cssText = [
    'background: rgba(0,0,0,0.08)',
    'border: 1px solid rgba(0,0,0,0.15)',
    'border-radius: 4px',
    'padding: 2px 8px',
    'font-size: 11px',
    'cursor: pointer',
    'color: inherit',
  ].join(';');
  btn.addEventListener('click', onClick);
  return btn;
}
