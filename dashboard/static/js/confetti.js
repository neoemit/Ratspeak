// rsConfetti({ x, y, colors, count, duration }): canvas-based burst,
// auto-cleans after `duration` ms. Defaults: center of viewport, theme
// greens + accent, 40 particles (clamped 10..80), 1600ms.

(function() {
    'use strict';

    var _active = false;

    function _resolveColors(custom) {
        if (custom && custom.length) return custom.slice();
        var cs = getComputedStyle(document.documentElement);
        var green = (cs.getPropertyValue('--status-online') || '#2E8B57').trim();
        var accent = (cs.getPropertyValue('--accent') || '#D2693B').trim();
        var warn = (cs.getPropertyValue('--status-warning') || '#D4A72C').trim();
        return [green, accent, warn, '#ffffff'];
    }

    function rsConfetti(opts) {
        if (_active) return;
        if (typeof document === 'undefined' || !document.body) return;
        var prefersReduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
        if (prefersReduce) return;

        opts = opts || {};
        var count = Math.max(10, Math.min(80, opts.count || 40));
        var duration = Math.max(600, Math.min(4000, opts.duration || 1600));
        var colors = _resolveColors(opts.colors);

        var w = window.innerWidth;
        var h = window.innerHeight;
        var originX = (typeof opts.x === 'number') ? opts.x : w / 2;
        var originY = (typeof opts.y === 'number') ? opts.y : h / 2.4;

        var canvas = document.createElement('canvas');
        canvas.width = w;
        canvas.height = h;
        canvas.style.cssText = 'position:fixed;inset:0;width:100vw;height:100vh;pointer-events:none;z-index:10000;';
        canvas.setAttribute('aria-hidden', 'true');
        document.body.appendChild(canvas);
        _active = true;

        var ctx = canvas.getContext('2d');
        var particles = [];
        for (var i = 0; i < count; i++) {
            var angle = (Math.PI * 2) * (i / count) + (Math.random() - 0.5) * 0.6;
            var speed = 4 + Math.random() * 5;
            particles.push({
                x: originX,
                y: originY,
                vx: Math.cos(angle) * speed,
                vy: Math.sin(angle) * speed - 3,
                rot: Math.random() * Math.PI,
                vr: (Math.random() - 0.5) * 0.3,
                size: 6 + Math.random() * 5,
                color: colors[i % colors.length],
                shape: i % 2
            });
        }

        var startedAt = performance.now();
        var gravity = 0.18;
        var drag = 0.992;

        function frame(now) {
            var elapsed = now - startedAt;
            var life = 1 - (elapsed / duration);
            if (life <= 0) {
                canvas.parentNode && canvas.parentNode.removeChild(canvas);
                _active = false;
                return;
            }
            ctx.clearRect(0, 0, w, h);
            ctx.globalAlpha = Math.max(0, Math.min(1, life * 1.2));
            for (var j = 0; j < particles.length; j++) {
                var p = particles[j];
                p.vx *= drag;
                p.vy = p.vy * drag + gravity;
                p.x += p.vx;
                p.y += p.vy;
                p.rot += p.vr;
                ctx.save();
                ctx.translate(p.x, p.y);
                ctx.rotate(p.rot);
                ctx.fillStyle = p.color;
                if (p.shape === 0) {
                    ctx.fillRect(-p.size / 2, -p.size / 3, p.size, p.size * 0.6);
                } else {
                    ctx.beginPath();
                    ctx.arc(0, 0, p.size / 2, 0, Math.PI * 2);
                    ctx.fill();
                }
                ctx.restore();
            }
            requestAnimationFrame(frame);
        }
        requestAnimationFrame(frame);
    }

    window.rsConfetti = rsConfetti;
})();
