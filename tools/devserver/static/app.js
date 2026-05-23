// Inputx Dev Inspector — vanilla JS, no build step.

// ---- Tab switching --------------------------------------------------------
const tabs = document.querySelectorAll('.tab');
const panels = document.querySelectorAll('.panel');
function switchTab(name) {
  tabs.forEach(t => t.classList.toggle('active', t.dataset.tab === name));
  panels.forEach(p => p.classList.toggle('active', p.id === name));
  if (name === 'heteronyms') loadHeteronyms();
  if (name === 'polish') loadPolish();
}
tabs.forEach(t => {
  t.addEventListener('click', e => {
    e.preventDefault();
    switchTab(t.dataset.tab);
    history.replaceState(null, '', '#' + t.dataset.tab);
  });
});
// Initial tab from hash
const hash = window.location.hash.replace('#', '');
if (hash && document.getElementById(hash)) switchTab(hash);

// ---- Probe tab (live) -----------------------------------------------------
const probeQ = document.getElementById('probe-q');
const probeMode = document.getElementById('probe-mode');
const probeJp = document.getElementById('probe-jp');
const probeMeta = document.getElementById('probe-meta');
const probeBody = document.querySelector('#probe-table tbody');

let probeDebounce = null;
function scheduleProbe() {
  if (probeDebounce) clearTimeout(probeDebounce);
  probeDebounce = setTimeout(runProbe, 80);
}

async function runProbe() {
  const q = probeQ.value.trim();
  if (!q) {
    probeMeta.textContent = '';
    probeBody.innerHTML = '';
    return;
  }
  const params = new URLSearchParams({
    q,
    mode: probeMode.value,
    jp: probeJp.checked ? '1' : '0',
  });
  try {
    const r = await fetch('/api/probe?' + params.toString());
    if (!r.ok) throw new Error('http ' + r.status);
    const data = await r.json();
    renderProbe(data);
  } catch (e) {
    probeMeta.textContent = 'error: ' + e.message;
  }
}

function renderProbe(data) {
  probeMeta.textContent = `mode=${data.mode}  jp=${data.japaneseEnabled}  preedit="${data.preedit}"  (${data.candidates.length} candidates)`;
  probeBody.innerHTML = '';
  data.candidates.forEach((c, i) => {
    const tr = document.createElement('tr');
    const numCell = document.createElement('td'); numCell.textContent = (i + 1).toString();
    const wordCell = document.createElement('td'); wordCell.textContent = c.word; wordCell.className = 'candidate-cell';
    const srcCell = document.createElement('td'); srcCell.textContent = c.source; srcCell.className = 'src-' + c.source;
    tr.appendChild(numCell);
    tr.appendChild(wordCell);
    tr.appendChild(srcCell);
    probeBody.appendChild(tr);
  });
}

probeQ.addEventListener('input', scheduleProbe);
probeMode.addEventListener('change', runProbe);
probeJp.addEventListener('change', runProbe);

// ---- Weights tab ----------------------------------------------------------
const weightsQ = document.getElementById('weights-q');
const weightsBody = document.querySelector('#weights-table tbody');

let weightsDebounce = null;
weightsQ.addEventListener('input', () => {
  if (weightsDebounce) clearTimeout(weightsDebounce);
  weightsDebounce = setTimeout(runWeights, 120);
});

async function runWeights() {
  const q = weightsQ.value.trim();
  if (!q) {
    weightsBody.innerHTML = '';
    return;
  }
  const r = await fetch('/api/weights?q=' + encodeURIComponent(q));
  const data = await r.json();
  weightsBody.innerHTML = '';
  data.rows.forEach(row => {
    const tr = document.createElement('tr');
    const c = document.createElement('td'); c.textContent = row.code; c.style.fontFamily = 'monospace';
    const w = document.createElement('td'); w.textContent = row.word; w.className = 'candidate-cell';
    const f = document.createElement('td'); f.textContent = row.freq;
    tr.appendChild(c); tr.appendChild(w); tr.appendChild(f);
    weightsBody.appendChild(tr);
  });
}

// ---- Heteronyms tab -------------------------------------------------------
const heteroQ = document.getElementById('hetero-q');
const heteroBody = document.querySelector('#hetero-table tbody');
const heteroCount = document.getElementById('hetero-count');
let allHetero = [];

async function loadHeteronyms() {
  if (allHetero.length) { renderHetero(allHetero); return; }
  const r = await fetch('/api/heteronyms');
  const data = await r.json();
  allHetero = data.rows;
  heteroCount.textContent = `${data.count} 条`;
  renderHetero(allHetero);
}
function renderHetero(rows) {
  heteroBody.innerHTML = '';
  rows.slice(0, 500).forEach(row => {
    const tr = document.createElement('tr');
    const p = document.createElement('td'); p.textContent = row.phrase; p.className = 'candidate-cell';
    const c = document.createElement('td'); c.textContent = row.canonical; c.style.fontFamily = 'monospace';
    tr.appendChild(p); tr.appendChild(c);
    heteroBody.appendChild(tr);
  });
}
heteroQ.addEventListener('input', () => {
  const q = heteroQ.value.trim().toLowerCase();
  if (!q) { renderHetero(allHetero); return; }
  const filtered = allHetero.filter(r =>
    r.phrase.includes(q) || r.canonical.toLowerCase().includes(q)
  );
  renderHetero(filtered);
});

// ---- Polish-log tab -------------------------------------------------------
async function loadPolish() {
  const r = await fetch('/api/polish-log');
  const data = await r.json();
  document.getElementById('polish-total').textContent = data.total ?? 0;

  const aggBody = document.querySelector('#polish-agg tbody');
  aggBody.innerHTML = '';
  (data.aggregate ?? []).forEach(a => {
    const tr = document.createElement('tr');
    const b = document.createElement('td'); b.textContent = a.buffer; b.style.fontFamily = 'monospace';
    const p = document.createElement('td'); p.textContent = a.preferred; p.className = 'candidate-cell';
    const c = document.createElement('td'); c.textContent = a.count;
    tr.appendChild(b); tr.appendChild(p); tr.appendChild(c);
    aggBody.appendChild(tr);
  });

  const recentBody = document.querySelector('#polish-recent tbody');
  recentBody.innerHTML = '';
  (data.recent ?? []).forEach(e => {
    const tr = document.createElement('tr');
    const ts = document.createElement('td');
    ts.textContent = (e.ts || '').replace('T', ' ').replace(/\..+/, '');
    ts.style.fontFamily = 'monospace';
    ts.style.fontSize = '11px';
    const buf = document.createElement('td'); buf.textContent = e.buffer || ''; buf.style.fontFamily = 'monospace';
    const cands = document.createElement('td');
    cands.textContent = (e.candidates || []).slice(0, 3).join(' / ');
    const picked = document.createElement('td'); picked.textContent = e.pickedWord || '';
    picked.className = 'candidate-cell';
    const idx = document.createElement('td'); idx.textContent = e.pickedIdx;
    tr.appendChild(ts); tr.appendChild(buf); tr.appendChild(cands); tr.appendChild(picked); tr.appendChild(idx);
    recentBody.appendChild(tr);
  });
}

// Pre-load polish on startup so the tab is ready when clicked.
loadPolish();
