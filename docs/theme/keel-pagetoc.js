// "On this page" right-rail table of contents.
// Builds a nav from the h2/h3 headings in <main>, highlights the section
// currently in view, and only shows on wide viewports (see keel-docs.css —
// it is display:none below 1500px so it never overlaps the centered body).
(() => {
    const build = () => {
        const main = document.querySelector('#mdbook-content main, .content main');
        if (!main) {
            return;
        }

        const headings = Array.from(main.querySelectorAll('h2[id], h3[id]'));
        if (headings.length < 2) {
            return;
        }

        const nav = document.createElement('nav');
        nav.className = 'keel-pagetoc';
        nav.setAttribute('aria-label', 'On this page');

        const title = document.createElement('div');
        title.className = 'keel-pagetoc-title';
        title.textContent = 'On this page';
        nav.appendChild(title);

        const list = document.createElement('ul');
        const linkFor = new Map();
        for (const h of headings) {
            const li = document.createElement('li');
            li.className = 'keel-pagetoc-' + h.tagName.toLowerCase();
            const a = document.createElement('a');
            a.href = '#' + h.id;
            a.textContent = h.textContent.replace(/¶/g, '').trim();
            li.appendChild(a);
            list.appendChild(li);
            linkFor.set(h.id, a);
        }
        nav.appendChild(list);
        main.appendChild(nav);

        // Scroll-spy: mark the heading nearest the top of the viewport active.
        let active = null;
        const setActive = (id) => {
            if (id === active) {
                return;
            }
            if (active && linkFor.has(active)) {
                linkFor.get(active).classList.remove('active');
            }
            active = id;
            if (active && linkFor.has(active)) {
                linkFor.get(active).classList.add('active');
            }
        };

        const visible = new Set();
        const observer = new IntersectionObserver((entries) => {
            for (const entry of entries) {
                if (entry.isIntersecting) {
                    visible.add(entry.target.id);
                } else {
                    visible.delete(entry.target.id);
                }
            }
            // Pick the first heading (document order) that is currently visible.
            for (const h of headings) {
                if (visible.has(h.id)) {
                    setActive(h.id);
                    return;
                }
            }
        }, { rootMargin: '-80px 0px -70% 0px', threshold: 0 });

        for (const h of headings) {
            observer.observe(h);
        }
    };

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', build);
    } else {
        build();
    }
})();
