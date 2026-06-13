// Keel docs ship exactly two themes — Keel Dark and Keel Light — both
// defined in keel-docs.css. mdbook's built-in picker lists its five stock
// themes (Light, Rust, Coal, Navy, Ayu). Rather than pin a full index.hbs
// override against a specific mdbook version, prune and relabel the menu at
// runtime: keep Auto + the two themes our CSS actually styles, drop the rest.
//
//   Keel Dark  -> mdbook-theme-navy   (aliased to .keel in CSS)
//   Keel Light -> mdbook-theme-light
(() => {
    const KEEP = {
        'mdbook-theme-default_theme': 'Auto',
        'mdbook-theme-navy': 'Keel Dark',
        'mdbook-theme-light': 'Keel Light',
    };

    const prune = () => {
        const list = document.getElementById('mdbook-theme-list');
        if (!list) {
            return false;
        }
        for (const li of Array.from(list.querySelectorAll('li'))) {
            const button = li.querySelector('button.theme');
            const label = button && KEEP[button.id];
            if (label) {
                button.textContent = label;
            } else {
                li.remove();
            }
        }
        return true;
    };

    if (!prune()) {
        document.addEventListener('DOMContentLoaded', prune);
    }
})();
