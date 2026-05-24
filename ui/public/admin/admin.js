// Shared helpers used by every admin page. Exposed on `window.gpa` so the
// per-page `x-data` factories can call them without re-importing.
(function () {
  const tokens = window.__GAPES_ADMIN_TOKENS || {};

  function pickToken(verb) {
    // verb in { read, admin, manage, destroy }. We hand the page server-side
    // pre-minted access tokens for the verbs we expect to use; if a verb is
    // unknown we fall back to the broadest token we have.
    return tokens[verb] || tokens.admin || tokens.read || '';
  }

  async function request(path, method, body, verb) {
    const tok = pickToken(verb);
    const opts = {
      method,
      headers: {
        'accept': 'application/json',
        ...(tok ? { 'authorization': 'Bearer ' + tok } : {}),
        ...(body !== null && body !== undefined ? { 'content-type': 'application/json' } : {})
      },
      credentials: 'same-origin'
    };
    if (body !== null && body !== undefined) opts.body = JSON.stringify(body);
    const resp = await fetch(path, opts);
    if (resp.status === 204) return null;
    const ct = resp.headers.get('content-type') || '';
    const data = ct.includes('json') ? await resp.json().catch(() => null) : null;
    if (!resp.ok) {
      const msg = (data && (data.message || data.error)) || ('http ' + resp.status);
      throw new Error(msg);
    }
    return data;
  }

  function fmtBytes(n) {
    if (n == null) return '—';
    const u = ['B', 'KB', 'MB', 'GB'];
    let i = 0, v = Number(n);
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return v.toFixed(v >= 100 || i === 0 ? 0 : 1) + ' ' + u[i];
  }

  function fmtTime(ts) {
    if (!ts) return '—';
    const d = new Date(Number(ts) * 1000);
    if (isNaN(d.getTime())) return '—';
    const pad = (x) => String(x).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  window.gpa = {
    apiGet: (path, verb) => request(path, 'GET', null, verb),
    apiJson: (path, method, body, verb) => request(path, method, body, verb),
    fmtBytes,
    fmtTime
  };
})();
