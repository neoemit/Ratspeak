// Delegated event handlers — replaces inline `onclick=` so CSP can drop
// `'unsafe-inline'` from script-src. Delegation also reaches elements
// mounted via innerHTML after this file loads.

(function () {
    'use strict';

    document.addEventListener('click', function (ev) {
        // Stop bubble so the section header doesn't toggle expand/collapse.
        var actionBtn = ev.target.closest('.conn-section-action');
        if (actionBtn) {
            ev.stopPropagation();
            return;
        }

        var kebab = ev.target.closest('.ble-peer-kebab');
        if (kebab) {
            if (typeof showBlePeerActions === 'function') {
                showBlePeerActions(ev, kebab);
            }
            return;
        }

        var header = ev.target.closest('.conn-section-header');
        if (header) {
            if (typeof toggleConnSection === 'function') {
                toggleConnSection(header);
            }
        }
    });

    document.addEventListener('keydown', function (ev) {
        var header = ev.target.closest('.conn-section-header');
        if (header) {
            if (typeof handleConnSectionKey === 'function') {
                handleConnSectionKey(ev);
            }
        }
    });
})();
