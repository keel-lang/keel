// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Mermaid is ~2.5 MB, so it is not bundled on every page via additional-js.
// This shim loads it on demand, only when the page contains a diagram.
(() => {
    if (!document.querySelector('pre.mermaid, .mermaid')) {
        return;
    }

    const darkThemes = ['ayu', 'navy', 'coal'];
    const lightThemes = ['light', 'rust'];

    const classList = document.getElementsByTagName('html')[0].classList;

    let lastThemeWasLight = true;
    for (const cssClass of classList) {
        if (darkThemes.includes(cssClass)) {
            lastThemeWasLight = false;
            break;
        }
    }

    const root = typeof path_to_root !== 'undefined' ? path_to_root : '';
    const script = document.createElement('script');
    script.src = root + 'mermaid.min.js';
    script.onload = () => {
        const theme = lastThemeWasLight ? 'default' : 'dark';
        mermaid.initialize({ startOnLoad: true, theme });
    };
    document.head.appendChild(script);

    // Simplest way to make mermaid re-render the diagrams in the new theme is via refreshing the page

    // mdbook 0.5 prefixes theme button ids with `mdbook-theme-`.
    for (const darkTheme of darkThemes) {
        document.getElementById('mdbook-theme-' + darkTheme)?.addEventListener('click', () => {
            if (lastThemeWasLight) {
                window.location.reload();
            }
        });
    }

    for (const lightTheme of lightThemes) {
        document.getElementById('mdbook-theme-' + lightTheme)?.addEventListener('click', () => {
            if (!lastThemeWasLight) {
                window.location.reload();
            }
        });
    }
})();
