// Keel syntax highlighting for highlight.js (used by mdBook)
hljs.registerLanguage('keel', function(hljs) {
  const KEYWORDS = {
    keyword: [
      'agent', 'task', 'role', 'model', 'tools', 'connect', 'memory', 'state', 'config',
      'type', 'run', 'stop', 'use', 'from', 'extern',
      'every', 'after', 'at', 'wait',
      'if', 'else', 'when', 'for', 'in', 'where', 'return', 'try', 'catch', 'retry', 'then',
      'classify', 'extract', 'summarize', 'draft', 'translate', 'decide', 'prompt',
      'as', 'considering', 'fallback', 'using', 'format',
      'ask', 'confirm', 'notify', 'show',
      'fetch', 'send', 'search', 'archive',
      'delegate', 'broadcast', 'team',
      'remember', 'recall', 'forget',
      'to', 'via', 'with', 'on', 'options',
      'parallel', 'race', 'set',
      'and', 'or', 'not',
      'rules', 'limits', 'times', 'backoff',
    ],
    literal: [
      'true', 'false', 'none', 'now',
      'persistent', 'session', 'background',
    ],
    type: [
      'str', 'int', 'float', 'bool', 'duration', 'datetime', 'dynamic',
      'list', 'map',
    ],
    built_in: [
      'self', 'env', 'user',
    ],
  };

  const STRING = {
    scope: 'string',
    begin: '"',
    end: '"',
    contains: [
      hljs.BACKSLASH_ESCAPE,
      {
        scope: 'subst',
        begin: '\\{',
        end: '\\}',
        contains: [
          { scope: 'variable', match: /[a-zA-Z_][a-zA-Z0-9_.]*/ },
        ],
      },
    ],
  };

  const NUMBER = {
    scope: 'number',
    variants: [
      { match: /\b[0-9]+\.[0-9]+\b/ },
      { match: /\b[0-9]+\b/ },
    ],
  };

  const COMMENT = hljs.COMMENT('#', '$');

  const OPERATOR = {
    scope: 'operator',
    match: /=>|->|\|>|==|!=|<=|>=|\?\?|\?\.|[=+\-*\/%<>!|]/,
  };

  const TYPE_NAME = {
    scope: 'title.class',
    match: /\b[A-Z][a-zA-Z0-9_]*\b/,
  };

  const DURATION = {
    scope: 'number',
    match: /\b[0-9]+\.(seconds?|minutes?|hours?|days?|sec|min|hr|[smhd])\b/,
  };

  const FUNCTION_CALL = {
    scope: 'title.function',
    match: /\b[a-z_][a-zA-Z0-9_]*(?=\s*\()/,
  };

  return {
    name: 'Keel',
    aliases: ['keel'],
    keywords: KEYWORDS,
    contains: [
      COMMENT,
      STRING,
      DURATION,
      NUMBER,
      OPERATOR,
      TYPE_NAME,
      FUNCTION_CALL,
    ],
  };
});

// Re-highlight all Keel code blocks.
// mdBook ships highlight.js 10.x which uses highlightBlock, not highlightElement.
(function() {
  var doHighlight = hljs.highlightElement || hljs.highlightBlock;

  function highlightKeel() {
    document.querySelectorAll('pre code.language-keel').forEach(function(block) {
      // Strip any failed previous attempt
      block.className = 'language-keel';
      doHighlight.call(hljs, block);
    });
  }

  // Run after DOM is ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', highlightKeel);
  } else {
    highlightKeel();
  }
})();
