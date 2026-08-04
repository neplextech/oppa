/** Development browser UI served at GET /dev. */
export const DEV_UI_HTML = /* html */ `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>OpenPrinter Dev Server</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: ui-monospace, 'Cascadia Code', 'Source Code Pro', Menlo, Consolas, monospace;
      background: #0d1117;
      color: #c9d1d9;
      padding: 32px 24px;
      max-width: 860px;
    }
    h1 { font-size: 18px; font-weight: 600; color: #e6edf3; margin-bottom: 4px; }
    .subtitle { font-size: 13px; color: #8b949e; margin-bottom: 28px; }
    .card { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 20px; margin-bottom: 16px; }
    .card-title { font-size: 11px; font-weight: 600; color: #8b949e; text-transform: uppercase; letter-spacing: 0.06em; margin-bottom: 16px; }
    .section-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
    .section-header .card-title { margin-bottom: 0; }
    .info-grid { display: grid; grid-template-columns: max-content 1fr; gap: 7px 24px; font-size: 13px; }
    .info-label { color: #8b949e; }
    .info-value { color: #58a6ff; word-break: break-all; }
    .deeplink-box {
      background: #0d1117; border: 1px solid #30363d; border-radius: 6px;
      padding: 12px 14px; font-size: 12px; color: #7ee787;
      word-break: break-all; line-height: 1.7; margin-bottom: 12px;
    }
    .pair-code { font-size: 26px; font-weight: 700; letter-spacing: 0.12em; color: #ffa657; margin-bottom: 4px; }
    .expiry { font-size: 12px; color: #8b949e; margin-bottom: 16px; }
    .btn-row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
    .btn {
      background: #21262d; border: 1px solid #30363d; color: #c9d1d9;
      border-radius: 6px; padding: 6px 14px; font-size: 12px; font-family: inherit;
      cursor: pointer; text-decoration: none; display: inline-block;
      transition: background 0.12s, border-color 0.12s;
    }
    .btn:hover { background: #30363d; }
    .btn-blue { background: #1f6feb; border-color: #388bfd; color: #fff; }
    .btn-blue:hover { background: #388bfd; }
    .btn-green { background: #238636; border-color: #2ea043; color: #fff; }
    .btn-green:hover { background: #2ea043; }
    .btn-sm { padding: 4px 10px; font-size: 11px; }
    .agent-list { display: flex; flex-direction: column; gap: 10px; }
    .agent-card { background: #0d1117; border: 1px solid #30363d; border-radius: 6px; padding: 14px; }
    .agent-id { font-size: 13px; font-weight: 600; color: #ffa657; margin-bottom: 6px; }
    .agent-meta { font-size: 12px; color: #8b949e; margin-bottom: 10px; line-height: 1.6; }
    .printer-list { display: flex; flex-direction: column; gap: 6px; }
    .printer-row {
      display: flex; align-items: center; justify-content: space-between;
      background: #161b22; border: 1px solid #30363d; border-radius: 4px; padding: 7px 10px;
      font-size: 12px; gap: 8px;
    }
    .printer-info { display: flex; align-items: center; gap: 8px; min-width: 0; }
    .printer-name { color: #c9d1d9; }
    .printer-kind { color: #6e7681; }
    .badge {
      display: inline-block; border-radius: 10px; padding: 1px 7px;
      font-size: 11px; white-space: nowrap;
    }
    .badge-green { background: #1a3d20; color: #7ee787; border: 1px solid #2ea043; }
    .badge-gray { background: #21262d; color: #8b949e; border: 1px solid #30363d; }
    .empty { color: #8b949e; font-size: 13px; text-align: center; padding: 20px 0; }
    .flash-msg { font-size: 12px; color: #7ee787; }
  </style>
</head>
<body>
  <h1>OpenPrinter Dev Server</h1>
  <p class="subtitle">Development dashboard &middot; volatile in-memory store &middot; not for production</p>

  <div class="card">
    <div class="card-title">Server paths</div>
    <div class="info-grid">
      <span class="info-label">Base URL</span><span class="info-value" id="val-base">—</span>
      <span class="info-label">Discovery</span><span class="info-value" id="val-discovery">—</span>
      <span class="info-label">Pairing</span><span class="info-value" id="val-pairing">—</span>
      <span class="info-label">Gateway</span><span class="info-value" id="val-gateway">—</span>
    </div>
  </div>

  <div class="card">
    <div class="card-title">Deep link pairing</div>
    <div id="pairing-area"><div class="empty">Generating code…</div></div>
    <div class="btn-row" style="margin-top:14px">
      <button class="btn" onclick="generateCode()">New code</button>
      <span id="copy-msg" class="flash-msg" style="display:none">Copied!</span>
    </div>
  </div>

  <div class="card">
    <div class="section-header">
      <div class="card-title">Connected agents</div>
      <button class="btn btn-sm" onclick="loadAgents()">Refresh</button>
    </div>
    <div id="agents-area"><div class="empty">Loading…</div></div>
  </div>

  <script>
    const BASE = location.origin;
    let currentCode = null;
    let currentExpiry = null;

    function escHtml(v) {
      return String(v).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
    }

    function pairLink(code) {
      const enc = btoa(BASE + '/').replace(/\\+/g,'-').replace(/\\//g,'_').replace(/=+$/,'');
      return 'oppa-dev://pair?server=' + enc + '&key=' + encodeURIComponent(code);
    }

    async function init() {
      document.getElementById('val-base').textContent = BASE;
      document.getElementById('val-discovery').textContent = BASE + '/.well-known/openprinter/info';
      document.getElementById('val-pairing').textContent = BASE + '/.well-known/openprinter/pair';
      document.getElementById('val-gateway').textContent = BASE.replace(/^http/, 'ws') + '/.well-known/openprinter/gateway';
      await Promise.all([generateCode(), loadAgents()]);
      setInterval(loadAgents, 10_000);
    }

    async function generateCode() {
      document.getElementById('pairing-area').innerHTML = '<div class="empty">Generating…</div>';
      try {
        const res = await fetch('/development/pairing-code', { method: 'POST' });
        if (!res.ok) throw new Error(res.statusText);
        const data = await res.json();
        currentCode = data.code;
        currentExpiry = new Date(data.expiresAt);
        renderPairing();
      } catch(e) {
        document.getElementById('pairing-area').innerHTML =
          '<div class="empty" style="color:#f85149">Failed to generate pairing code: ' + escHtml(e.message) + '</div>';
      }
    }

    function renderPairing() {
      const link = pairLink(currentCode);
      const secsLeft = Math.round((currentExpiry - Date.now()) / 1000);
      const minsLeft = Math.max(0, Math.round(secsLeft / 60));
      const expiryStr = currentExpiry.toLocaleTimeString();
      document.getElementById('pairing-area').innerHTML = \`
        <div class="pair-code">\${escHtml(currentCode)}</div>
        <div class="expiry">Expires ~\${minsLeft} min &middot; \${escHtml(expiryStr)}</div>
        <div class="deeplink-box" id="deeplink-val">\${escHtml(link)}</div>
        <div class="btn-row">
          <a class="btn btn-blue" href="\${escHtml(link)}" id="open-btn">Open in OPPA</a>
          <button class="btn" onclick="copyLink()">Copy link</button>
        </div>
      \`;
    }

    async function copyLink() {
      await navigator.clipboard.writeText(pairLink(currentCode));
      const el = document.getElementById('copy-msg');
      el.style.display = 'inline';
      setTimeout(() => { el.style.display = 'none'; }, 2000);
    }

    async function loadAgents() {
      try {
        const res = await fetch('/agents');
        if (!res.ok) throw new Error(res.statusText);
        const agents = await res.json();
        renderAgents(agents);
      } catch(e) {
        document.getElementById('agents-area').innerHTML =
          '<div class="empty" style="color:#f85149">Failed to load agents: ' + escHtml(e.message) + '</div>';
      }
    }

    function renderAgents(agents) {
      if (!agents.length) {
        document.getElementById('agents-area').innerHTML =
          '<div class="empty">No agents connected. Use the deep link above to pair OPPA.</div>';
        return;
      }
      const html = agents.map(a => \`
        <div class="agent-card">
          <div class="agent-id">\${escHtml(a.agentId)}</div>
          <div class="agent-meta">
            product: \${escHtml(a.productId)} &nbsp;&middot;&nbsp;
            version: \${escHtml(a.agentVersion)} &nbsp;&middot;&nbsp;
            connected: \${escHtml(new Date(a.connectedAt).toLocaleTimeString())}
          </div>
          <div class="printer-list" id="pl-\${escHtml(a.agentId)}">
            <div style="font-size:12px;color:#8b949e">Loading printers…</div>
          </div>
        </div>
      \`).join('');
      document.getElementById('agents-area').innerHTML = '<div class="agent-list">' + html + '</div>';
      for (const a of agents) loadPrinters(a.agentId);
    }

    async function loadPrinters(agentId) {
      try {
        const res = await fetch('/agents/' + encodeURIComponent(agentId) + '/printers');
        if (!res.ok) throw new Error(res.statusText);
        const printers = await res.json();
        const el = document.getElementById('pl-' + CSS.escape(agentId));
        if (!el) return;
        if (!printers.length) {
          el.innerHTML = '<div style="font-size:12px;color:#8b949e">No printers reported yet. Try refreshing.</div>';
          return;
        }
        el.innerHTML = printers.map(p => \`
          <div class="printer-row">
            <div class="printer-info">
              <span class="printer-name">\${escHtml(p.name)}</span>
              <span class="printer-kind">\${escHtml(p.kind)}</span>
              <span class="badge \${p.enabled ? 'badge-green' : 'badge-gray'}">\${p.enabled ? 'enabled' : 'disabled'}</span>
            </div>
            <button class="btn btn-green btn-sm"
              data-agent="\${escHtml(agentId)}"
              data-printer="\${escHtml(p.fingerprint)}"
              onclick="handleTestPrint(this)">Test print</button>
          </div>
        \`).join('');
      } catch(e) {
        console.error('printers load failed for', agentId, e);
      }
    }

    function handleTestPrint(btn) {
      const agentId = btn.dataset.agent;
      const printerId = btn.dataset.printer;
      btn.disabled = true;
      btn.textContent = 'Sending…';
      fetch(
        '/development/test-print/' + encodeURIComponent(agentId) + '/' + encodeURIComponent(printerId),
        { method: 'POST' }
      )
        .then(r => r.json())
        .then(result => {
          btn.textContent = result.ok ? 'Sent ✓' : 'Failed ✗';
          setTimeout(() => { btn.disabled = false; btn.textContent = 'Test print'; }, 3000);
        })
        .catch(e => {
          btn.textContent = 'Error ✗';
          console.error(e);
          setTimeout(() => { btn.disabled = false; btn.textContent = 'Test print'; }, 3000);
        });
    }

    init();
  </script>
</body>
</html>`;
