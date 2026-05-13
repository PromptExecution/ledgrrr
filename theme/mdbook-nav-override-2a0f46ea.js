/**
 * Override mdbook's default chapter navigation to use Ctrl+Arrow keys instead of Arrow keys.
 * This prevents the live Rhai editor from losing focus when users press arrow keys.
 */
(function () {
    document.addEventListener('keydown', function (e) {
        // Only handle Ctrl+Left/Right, not plain arrow keys
        if (!e.ctrlKey) {
            return;
        }

        const prevNav = document.querySelector('a.nav-chapters.previous');
        const nextNav = document.querySelector('a.nav-chapters.next');

        if (e.key === 'ArrowLeft' && prevNav && prevNav.href) {
            e.preventDefault();
            window.location.href = prevNav.href;
        } else if (e.key === 'ArrowRight' && nextNav && nextNav.href) {
            e.preventDefault();
            window.location.href = nextNav.href;
        }
    });

    // Disable mdbook's default arrow key navigation by removing its event listeners
    // mdbook attaches navigation to document, so we stop propagation for plain arrow keys
    // when focus is inside a textarea, input, or contenteditable element
    document.addEventListener('keydown', function (e) {
        if (e.ctrlKey) return; // Let Ctrl+arrow through for our custom nav

        const target = e.target;
        const isEditor = target.tagName === 'TEXTAREA' ||
            target.tagName === 'INPUT' ||
            target.isContentEditable ||
            target.closest('.rhai-live-editor') ||
            target.closest('.rhai-live-core') ||
            target.closest('code.language-rhai');

        if (isEditor && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
            e.stopPropagation();
        }
    }, true); // Use capture phase to intercept before mdbook's handler
})();
