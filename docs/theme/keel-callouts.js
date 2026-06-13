// Semantic callouts. Docs author callouts as a blockquote whose first line is
// a bold label — `> **Note:** ...`, `> **Warning:** ...`. This classifies each
// blockquote by that leading word and tags it so keel-docs.css can colour the
// accent (info = blue, tip = green, warning = burnt amber). Unrecognised labels
// keep the neutral amber default.
(() => {
    const KIND = {
        note: 'info', info: 'info', latest: 'info', new: 'info',
        tip: 'tip', success: 'tip',
        warning: 'warn', caution: 'warn', breaking: 'warn', alpha: 'warn',
        danger: 'warn', deprecated: 'warn',
    };

    const classify = () => {
        const main = document.querySelector('#mdbook-content main, .content main');
        if (!main) {
            return;
        }
        for (const bq of main.querySelectorAll('blockquote')) {
            const strong = bq.querySelector('p:first-child > strong:first-child');
            if (!strong) {
                continue;
            }
            const word = strong.textContent.trim().toLowerCase().match(/[a-z]+/);
            const kind = word && KIND[word[0]];
            if (kind) {
                bq.classList.add('callout-' + kind);
            }
        }
    };

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', classify);
    } else {
        classify();
    }
})();
