// Radiance Music Server: what every page shares (the device's key, the API,
// icons, toasts, the look). The key is this browser's device token, kept in
// localStorage; files and the party's WebSocket take it as ?t=.
const RMS = (() => {
  const KEY = 'rms.token';
  const token = () => localStorage.getItem(KEY) || '';
  const setToken = (t) => (t ? localStorage.setItem(KEY, t) : localStorage.removeItem(KEY));

  // a key in the address (?t=…, from a link): kept, then taken out of the address
  const fromUrl = new URLSearchParams(location.search).get('t');
  if (fromUrl) {
    setToken(fromUrl);
    const u = new URL(location.href);
    u.searchParams.delete('t');
    history.replaceState(null, '', u.pathname + u.search + u.hash);
  }

  /** Calls the API: {ok, status, data}. status 0: the server couldn't be reached. */
  async function api(path, { method = 'GET', body, auth = true } = {}) {
    const headers = {};
    if (auth && token()) headers.Authorization = 'Bearer ' + token();
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    try {
      const res = await fetch(path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), cache: 'no-store' });
      let data = null;
      try { data = await res.json(); } catch (_) { data = {}; }
      return { ok: res.ok, status: res.status, data };
    } catch (_) {
      return { ok: false, status: 0, data: { error: "Can't reach the server. Check your connection; it tries again by itself." } };
    }
  }

  /** A library file's address, with the key (players can't send headers). */
  const fileUrl = (url) => (url ? url + (url.includes('?') ? '&' : '?') + 't=' + encodeURIComponent(token()) : '');

  // lucide's shapes (round ends and corners)
  const ICONS = {
    'user-plus': '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M19 8v6"/><path d="M22 11h-6"/>',
    'user-x': '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="m17 8 5 5"/><path d="m22 8-5 5"/>',
    users: '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>',
    copy: '<rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
    check: '<path d="M20 6 9 17l-5-5"/>',
    x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
    'x-circle': '<circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/>',
    trash: '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>',
    upload: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m17 8-5-5-5 5"/><path d="M12 3v12"/>',
    download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/>',
    split: '<path d="M2 10v3"/><path d="M6 6v11"/><path d="M10 3v18"/><path d="M14 8v7"/><path d="M18 5v13"/><path d="M22 10v3"/>',
    play: '<path d="M7 4.5v15a1 1 0 0 0 1.5.86l12-7.5a1 1 0 0 0 0-1.72l-12-7.5A1 1 0 0 0 7 4.5Z"/>',
    pause: '<rect x="14" y="4" width="4" height="16" rx="1.5"/><rect x="6" y="4" width="4" height="16" rx="1.5"/>',
    skip: '<path d="M5 5.5v13a1 1 0 0 0 1.6.8l9-6.5a1 1 0 0 0 0-1.6l-9-6.5A1 1 0 0 0 5 5.5Z"/><path d="M19 5v14"/>',
    headphones: '<path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a9 9 0 0 1 18 0v7a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3"/>',
    link: '<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>',
    qr: '<rect width="5" height="5" x="3" y="3" rx="1"/><rect width="5" height="5" x="16" y="3" rx="1"/><rect width="5" height="5" x="3" y="16" rx="1"/><path d="M21 16h-3a2 2 0 0 0-2 2v3"/><path d="M21 21v.01"/><path d="M12 7v3a2 2 0 0 1-2 2H7"/><path d="M3 12h.01"/><path d="M12 3h.01"/><path d="M12 16v.01"/><path d="M16 12h1"/><path d="M21 12v.01"/><path d="M12 21v-1"/>',
    refresh: '<path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>',
    'log-in': '<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><path d="m10 17 5-5-5-5"/><path d="M15 12H3"/>',
    'log-out': '<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><path d="m16 17 5-5-5-5"/><path d="M21 12H9"/>',
    plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
    up: '<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>',
    down: '<path d="M12 5v14"/><path d="m19 12-7 7-7-7"/>',
    music: '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
    phone: '<rect width="14" height="20" x="5" y="2" rx="2"/><path d="M12 18h.01"/>',
    globe: '<circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/><path d="M2 12h20"/>',
    search: '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>',
    volume: '<path d="M11 4.7a.7.7 0 0 0-1.2-.5L6.4 7.6A1.4 1.4 0 0 1 5.4 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.4a1.4 1.4 0 0 1 1 .4l3.4 3.4a.7.7 0 0 0 1.2-.5z"/><path d="M16 9a5 5 0 0 1 0 6"/><path d="M19.4 18.4a9 9 0 0 0 0-12.7"/>',
    server: '<rect width="20" height="8" x="2" y="2" rx="2"/><rect width="20" height="8" x="2" y="14" rx="2"/><path d="M6 6h.01"/><path d="M6 18h.01"/>',
    arrow: '<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>',
    loader: '<path d="M21 12a9 9 0 1 1-6.22-8.56"/>',
    eye: '<path d="M2.06 12.35a1 1 0 0 1 0-.7 10.75 10.75 0 0 1 19.88 0 1 1 0 0 1 0 .7 10.75 10.75 0 0 1-19.88 0"/><circle cx="12" cy="12" r="3"/>',
    heart: '<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>',
  };
  const icon = (name, cls = '') => `<svg class="i ${cls}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name] || ''}</svg>`;
  /** <span data-icon="x"></span> placeholders become their icons. */
  function hydrate(root = document) {
    root.querySelectorAll('[data-icon]').forEach((el) => {
      el.outerHTML = icon(el.dataset.icon, el.className);
    });
  }

  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

  let toastTimer;
  function toast(msg, bad = false) {
    let el = document.querySelector('.toast');
    if (!el) {
      el = document.createElement('div');
      el.className = 'toast';
      el.setAttribute('role', 'status');
      document.body.appendChild(el);
    }
    el.textContent = msg;
    el.classList.toggle('bad', !!bad);
    el.classList.remove('hidden');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => el.classList.add('hidden'), bad ? 6000 : 3500);
  }

  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text);
      toast('Copied the link.');
    } catch (_) {
      // no clipboard (http pages): the text, selected, to copy by hand
      window.prompt('Copy this link:', text);
    }
  }

  // the look: Stone or True Black, as in the app (kept per browser)
  function look() {
    const pick = (v) => {
      localStorage.setItem('rms.look', v);
      document.body.classList.toggle('black', v === 'black');
      document.querySelectorAll('.look button').forEach((b) => b.classList.toggle('sel', b.dataset.look === v));
    };
    document.querySelectorAll('.look').forEach((el) => {
      el.innerHTML =
        '<button data-look="stone" title="Stone: warm greys" aria-label="Stone"><span class="swatch" style="background:#44403C"></span>Stone</button>' +
        '<button data-look="black" title="True Black" aria-label="True Black"><span class="swatch" style="background:#000"></span>True Black</button>';
      el.querySelectorAll('button').forEach((b) => (b.onclick = () => pick(b.dataset.look)));
    });
    pick(localStorage.getItem('rms.look') === 'black' ? 'black' : 'stone');
  }

  const clock = (ms) => {
    if (ms == null || ms < 0 || !isFinite(ms)) return '0:00';
    const s = Math.floor(ms / 1000);
    return Math.floor(s / 60) + ':' + String(s % 60).padStart(2, '0');
  };
  function ago(ms) {
    if (!ms) return 'never';
    const s = Math.max(0, (Date.now() - ms) / 1000);
    if (s < 90) return 'just now';
    if (s < 3600) return Math.round(s / 60) + ' min ago';
    if (s < 86400) return Math.round(s / 3600) + ' h ago';
    return Math.round(s / 86400) + ' days ago';
  }
  const initial = (name) => esc((name || '?').trim().charAt(0).toUpperCase() || '?');
  const show = (el, on = true) => el && el.classList.toggle('hidden', !on);
  const $ = (sel, root = document) => root.querySelector(sel);

  document.addEventListener('DOMContentLoaded', () => {
    hydrate();
    look();
  });

  return { token, setToken, api, fileUrl, icon, hydrate, esc, toast, copy, clock, ago, initial, show, $ };
})();
