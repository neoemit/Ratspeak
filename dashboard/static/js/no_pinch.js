// Replaces viewport `user-scalable=no` (iOS Safari mostly ignores it and it
// fails WCAG 2.5.5). Opt-out via a `.lightbox-zoomable` ancestor so a future
// focused-image viewer can allow native pinch.

(function() {
    function isExempt(target) {
        if (!target || !target.closest) return false;
        return !!target.closest('.lightbox-zoomable');
    }

    function blockPinch(e) {
        if (e.touches && e.touches.length > 1 && !isExempt(e.target)) {
            e.preventDefault();
        }
    }
    document.addEventListener('touchmove', blockPinch, { passive: false });

    // iOS-specific gesture events fire even when touchmove is prevented.
    function blockGesture(e) {
        if (!isExempt(e.target)) e.preventDefault();
    }
    document.addEventListener('gesturestart', blockGesture);
    document.addEventListener('gesturechange', blockGesture);
    document.addEventListener('gestureend', blockGesture);

    // Catches edges where `touch-action: manipulation` isn't inherited.
    var lastTouchEnd = 0;
    document.addEventListener('touchend', function(e) {
        if (isExempt(e.target)) return;
        var now = Date.now();
        if (now - lastTouchEnd <= 300) e.preventDefault();
        lastTouchEnd = now;
    }, { passive: false });
})();
