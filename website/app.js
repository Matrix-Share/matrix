/* Lifeline site — progressive enhancement: reveal, nav, theme, counters, and the
   hero mesh animation (the "network is the people" metaphor). No dependencies. */
(function () {
  document.documentElement.classList.add('js');
  var reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  /* ---- Theme toggle (persisted; defaults to system) ---- */
  try {
    var saved = localStorage.getItem('ll-theme');
    if (saved) document.documentElement.setAttribute('data-theme', saved);
  } catch (e) {}
  window.toggleTheme = function () {
    var el = document.documentElement;
    var cur = el.getAttribute('data-theme');
    var isDark = cur ? cur === 'dark' : window.matchMedia('(prefers-color-scheme: dark)').matches;
    var next = isDark ? 'light' : 'dark';
    el.setAttribute('data-theme', next);
    try { localStorage.setItem('ll-theme', next); } catch (e) {}
  };

  /* ---- Mobile drawer ---- */
  window.toggleDrawer = function (open) {
    var d = document.getElementById('drawer');
    if (!d) return;
    d.classList.toggle('on', open);
    document.body.style.overflow = open ? 'hidden' : '';
  };

  function onReady(fn) {
    if (document.readyState !== 'loading') fn();
    else document.addEventListener('DOMContentLoaded', fn);
  }

  onReady(function () {
    /* Scroll reveal */
    var items = document.querySelectorAll('[data-reveal]');
    if (reduce || !('IntersectionObserver' in window)) {
      items.forEach(function (el) { el.classList.add('in'); });
    } else {
      var io = new IntersectionObserver(function (entries) {
        entries.forEach(function (en) {
          if (en.isIntersecting) { en.target.classList.add('in'); io.unobserve(en.target); }
        });
      }, { threshold: 0.14, rootMargin: '0px 0px -8% 0px' });
      items.forEach(function (el) { io.observe(el); });
    }

    /* Count-up stats */
    document.querySelectorAll('[data-count]').forEach(function (el) {
      var target = parseFloat(el.getAttribute('data-count'));
      var suffix = el.getAttribute('data-suffix') || '';
      if (reduce) { el.textContent = target + suffix; return; }
      var run = function () {
        var t0 = null, dur = 1300;
        function tick(ts) {
          if (!t0) t0 = ts;
          var p = Math.min(1, (ts - t0) / dur);
          var e = 1 - Math.pow(1 - p, 3);
          el.textContent = Math.round(target * e) + suffix;
          if (p < 1) requestAnimationFrame(tick);
        }
        requestAnimationFrame(tick);
      };
      if ('IntersectionObserver' in window) {
        var o = new IntersectionObserver(function (es) {
          es.forEach(function (en) { if (en.isIntersecting) { run(); o.unobserve(en.target); } });
        }, { threshold: 0.6 });
        o.observe(el);
      } else { el.textContent = target + suffix; }
    });

    /* Mesh hero animation */
    var canvas = document.getElementById('mesh');
    if (canvas) initMesh(canvas, reduce);
  });

  function accentRGB() {
    // Read the resolved accent for canvas strokes; fall back to indigo.
    var c = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
    var probe = document.createElement('span');
    probe.style.color = c || '#6366f1';
    document.body.appendChild(probe);
    var rgb = getComputedStyle(probe).color; // rgb(r,g,b)
    document.body.removeChild(probe);
    var m = rgb.match(/\d+/g);
    return m ? [ +m[0], +m[1], +m[2] ] : [99, 102, 241];
  }

  function initMesh(canvas, reduce) {
    var ctx = canvas.getContext('2d');
    var dpr = Math.min(2, window.devicePixelRatio || 1);
    var W = 0, H = 0, nodes = [], edges = [], packets = [];
    var rgb = accentRGB();
    var dark = document.documentElement.getAttribute('data-theme') === 'dark' ||
      (!document.documentElement.getAttribute('data-theme') && window.matchMedia('(prefers-color-scheme: dark)').matches);

    function resize() {
      var r = canvas.getBoundingClientRect();
      W = r.width; H = r.height;
      canvas.width = W * dpr; canvas.height = H * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      build();
    }
    function build() {
      var count = Math.round(Math.min(46, Math.max(16, (W * H) / 22000)));
      nodes = [];
      for (var i = 0; i < count; i++) {
        nodes.push({
          x: Math.random() * W, y: Math.random() * H,
          vx: (Math.random() - 0.5) * 0.14, vy: (Math.random() - 0.5) * 0.14,
          r: 1.4 + Math.random() * 1.8, pulse: Math.random() * Math.PI * 2
        });
      }
    }
    function computeEdges() {
      edges = [];
      var max = Math.min(W, H) * 0.22 + 90;
      for (var i = 0; i < nodes.length; i++) {
        for (var j = i + 1; j < nodes.length; j++) {
          var dx = nodes[i].x - nodes[j].x, dy = nodes[i].y - nodes[j].y;
          var d = Math.sqrt(dx * dx + dy * dy);
          if (d < max) edges.push({ a: i, b: j, d: d, max: max });
        }
      }
    }
    function neighbors(idx) {
      var out = [];
      edges.forEach(function (e) {
        if (e.a === idx) out.push(e.b); else if (e.b === idx) out.push(e.a);
      });
      return out;
    }
    // A packet performs a store-carry-forward walk across the mesh.
    function spawnPacket() {
      if (!nodes.length) return;
      var start = (Math.random() * nodes.length) | 0;
      packets.push({ node: start, from: start, t: 0, hops: 0, max: 5 + ((Math.random() * 4) | 0), trail: [] });
    }

    var last = 0, edgeTimer = 0;
    function frame(ts) {
      var dt = Math.min(40, ts - last); last = ts;
      ctx.clearRect(0, 0, W, H);
      edgeTimer -= dt;
      if (edgeTimer <= 0) { computeEdges(); edgeTimer = 260; }

      // drift
      nodes.forEach(function (n) {
        n.x += n.vx * dt; n.y += n.vy * dt; n.pulse += dt * 0.003;
        if (n.x < 0 || n.x > W) n.vx *= -1;
        if (n.y < 0 || n.y > H) n.vy *= -1;
      });

      // edges
      ctx.lineWidth = 1;
      edges.forEach(function (e) {
        var a = nodes[e.a], b = nodes[e.b];
        var alpha = (1 - e.d / e.max) * (dark ? 0.28 : 0.20);
        ctx.strokeStyle = 'rgba(' + rgb[0] + ',' + rgb[1] + ',' + rgb[2] + ',' + alpha + ')';
        ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
      });

      // nodes
      nodes.forEach(function (n) {
        var glow = 0.5 + 0.5 * Math.sin(n.pulse);
        ctx.beginPath();
        ctx.fillStyle = 'rgba(' + rgb[0] + ',' + rgb[1] + ',' + rgb[2] + ',' + (0.35 + glow * 0.35) + ')';
        ctx.arc(n.x, n.y, n.r + glow * 0.7, 0, Math.PI * 2); ctx.fill();
      });

      // packets hop node-to-node along edges
      for (var p = packets.length - 1; p >= 0; p--) {
        var pk = packets[p];
        pk.t += dt / 620;
        var a = nodes[pk.from], b = nodes[pk.node];
        if (!a || !b) { packets.splice(p, 1); continue; }
        var x = a.x + (b.x - a.x) * ease(pk.t), y = a.y + (b.y - a.y) * ease(pk.t);
        pk.trail.push({ x: x, y: y }); if (pk.trail.length > 14) pk.trail.shift();
        // trail
        for (var k = 0; k < pk.trail.length; k++) {
          var tp = pk.trail[k], tr = (k / pk.trail.length);
          ctx.beginPath();
          ctx.fillStyle = 'rgba(' + rgb[0] + ',' + rgb[1] + ',' + rgb[2] + ',' + (tr * 0.5) + ')';
          ctx.arc(tp.x, tp.y, 1.6 * tr + 0.4, 0, Math.PI * 2); ctx.fill();
        }
        // head
        ctx.beginPath();
        ctx.shadowBlur = 14; ctx.shadowColor = 'rgba(' + rgb[0] + ',' + rgb[1] + ',' + rgb[2] + ',0.9)';
        ctx.fillStyle = '#fff';
        ctx.arc(x, y, 2.6, 0, Math.PI * 2); ctx.fill();
        ctx.shadowBlur = 0;
        if (pk.t >= 1) {
          pk.from = pk.node; pk.hops++; pk.t = 0; pk.trail = [];
          var nb = neighbors(pk.node).filter(function (n) { return n !== pk.from || true; });
          if (pk.hops >= pk.max || !nb.length) { packets.splice(p, 1); }
          else { pk.node = nb[(Math.random() * nb.length) | 0]; }
        }
      }
      if (packets.length < 3 && Math.random() < 0.02) spawnPacket();
      raf = requestAnimationFrame(frame);
    }
    function ease(t) { return t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2; }

    var raf;
    resize();
    window.addEventListener('resize', debounce(resize, 200));
    if (reduce) {
      computeEdges();
      // one static frame
      ctx.clearRect(0, 0, W, H);
      edges.forEach(function (e) {
        var a = nodes[e.a], b = nodes[e.b];
        ctx.strokeStyle = 'rgba(' + rgb[0] + ',' + rgb[1] + ',' + rgb[2] + ',0.18)';
        ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
      });
      nodes.forEach(function (n) {
        ctx.fillStyle = 'rgba(' + rgb[0] + ',' + rgb[1] + ',' + rgb[2] + ',0.5)';
        ctx.beginPath(); ctx.arc(n.x, n.y, n.r, 0, Math.PI * 2); ctx.fill();
      });
    } else {
      for (var i = 0; i < 2; i++) spawnPacket();
      raf = requestAnimationFrame(frame);
    }
  }

  function debounce(fn, ms) {
    var t; return function () { clearTimeout(t); t = setTimeout(fn, ms); };
  }
})();
