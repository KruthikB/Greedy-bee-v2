const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const MIN_BOARD_TEXT = 20;
const MAX_BOARD_TEXT = 30;
const MAX_BOARD_LINK = 100;

const statusBadge = document.getElementById('status-badge');
const countdownLbl = document.getElementById('countdown-label');
const reminderList = document.getElementById('reminder-list');
const addBtn = document.getElementById('add-btn');
const resumeBtn = document.getElementById('resume-btn');
const quitBtn = document.getElementById('quit-btn');
const editorCard = document.getElementById('editor-card');
const editorError = document.getElementById('editor-error');
const aboutCredits = document.getElementById('about-credits');

let catalog = { characters: [], sharedActions: [] };
let reminders = [];
let activeReminderId = null;
let isPaused = false;
let formOpen = false;
let lastListKey = '';
const openIds = new Set();

function isBoardLink(s) {
  const t = String(s || '').trim().toLowerCase();
  return t.startsWith('http://') || t.startsWith('https://');
}

function boardLimits(value) {
  return isBoardLink(value)
    ? { min: 1, max: MAX_BOARD_LINK, label: MAX_BOARD_LINK }
    : { min: MIN_BOARD_TEXT, max: MAX_BOARD_TEXT, label: MAX_BOARD_TEXT };
}

async function init() {
  await loadCatalog();
  await listen('pause-changed', () => refreshStatus());
  await listen('overlay-dismissed', () => refreshStatus());
  await listen('reminders-changed', () => refreshStatus());

  await refreshStatus();
  setInterval(() => refreshStatus(), 1000);

  addBtn.addEventListener('click', () => openEditor(null));
  document.getElementById('save-btn').addEventListener('click', saveEditor);
  document.getElementById('cancel-btn').addEventListener('click', closeEditor);
  resumeBtn.addEventListener('click', async () => {
    await invoke('resume_reminders');
    await refreshStatus();
  });
  quitBtn.addEventListener('click', () => invoke('quit_app'));

  document.querySelectorAll('[data-pause]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const mins = parseInt(btn.dataset.pause, 10);
      // null = pause indefinitely; preserve each reminder's remaining delta on resume
      await invoke('pause_reminders', {
        durationMinutes: Number.isFinite(mins) && mins > 0 ? mins : null,
      });
      await refreshStatus();
    });
  });

  document.getElementById('edit-action').addEventListener('change', syncBoardField);
  document.getElementById('edit-schedule-type').addEventListener('change', syncSchedulePanes);
  document.getElementById('edit-message').addEventListener('input', () => {
    document.getElementById('msg-count').textContent =
      `${document.getElementById('edit-message').value.length}/80`;
  });
  document.getElementById('edit-board').addEventListener('input', updateBoardCounter);
}

async function loadCatalog() {
  try {
    catalog = await fetch('../assets/characters/catalog.json').then((r) => r.json());
  } catch {
    catalog = {
      characters: [
        { id: 'water-guy', name: 'Kaybie', type: 'video', actions: ['drink', 'board'] },
      ],
      sharedActions: [],
    };
  }
  const credits = (catalog.characters || [])
    .map((c) => (c.credit ? `${c.name} (${c.credit})` : c.name))
    .join(' · ');
  aboutCredits.textContent = credits ? `Characters: ${credits}` : aboutCredits.textContent;
}

function remainingOf(r) {
  const v = r.remainingSecs ?? r.remaining_secs;
  return v == null ? null : Number(v);
}

function timerLabel(r) {
  if (!r.enabled) return { text: 'Off', cls: 'disabled' };
  if (activeReminderId && r.id === activeReminderId) return { text: 'Now', cls: 'due' };
  const secs = remainingOf(r);
  if (secs == null) return { text: '—', cls: 'disabled' };
  if (!isPaused && secs <= 0) return { text: 'Due', cls: 'due' };
  return { text: fmt(secs), cls: '' };
}

async function refreshStatus() {
  let s;
  try {
    s = await invoke('get_status');
  } catch {
    return;
  }

  reminders = s.reminders || [];
  activeReminderId = s.activeReminderId ?? s.active_reminder_id ?? null;
  isPaused = !!(s.isPaused ?? s.is_paused);
  const pauseSecs = s.remainingPauseSecs ?? s.remaining_pause_secs ?? 0;
  const nextSecs = s.remainingReminderSecs ?? s.remaining_reminder_secs ?? 0;
  const nextName = s.nextReminderName ?? s.next_reminder_name ?? null;
  addBtn.disabled = reminders.length >= 10;

  if (isPaused) {
    statusBadge.textContent = '⏸  Paused';
    statusBadge.className = 'badge badge-paused';
    countdownLbl.textContent =
      pauseSecs < 0
        ? 'Paused indefinitely — timers frozen'
        : `Paused · resumes in ${fmt(pauseSecs)}`;
    resumeBtn.disabled = false;
  } else if (activeReminderId) {
    statusBadge.textContent = '●  Running';
    statusBadge.className = 'badge badge-running';
    const active = reminders.find((r) => r.id === activeReminderId);
    if (nextName && nextSecs > 0) {
      countdownLbl.textContent = `Showing ${active?.name || 'reminder'} · next ${nextName} in ${fmt(nextSecs)}`;
    } else {
      countdownLbl.textContent = `Showing ${active?.name || 'reminder'}…`;
    }
    resumeBtn.disabled = true;
  } else {
    statusBadge.textContent = '●  Running';
    statusBadge.className = 'badge badge-running';
    const name = nextName ? ` (${nextName})` : '';
    countdownLbl.textContent =
      nextSecs > 0 || nextName
        ? `Next reminder${name} in ${fmt(nextSecs)}`
        : 'No upcoming reminders';
    resumeBtn.disabled = true;
  }

    const listKey = JSON.stringify(
    reminders.map((r) => [
      r.id,
      r.enabled,
      r.name,
      r.scheduleSummary ?? r.schedule_summary,
      r.character,
      r.action,
      isPaused,
    ]),
  );
  if (!formOpen && listKey !== lastListKey) {
    lastListKey = listKey;
    renderList();
  } else if (!formOpen) {
    updateLiveTimers();
  }
}

function updateLiveTimers() {
  for (const r of reminders) {
    const el = reminderList.querySelector(`[data-timer="${r.id}"]`);
    if (!el) continue;
    const { text, cls } = timerLabel(r);
    el.textContent = text;
    el.className = `reminder-timer${cls ? ` ${cls}` : ''}`;
  }
  reminderList.querySelectorAll('.reminder-item').forEach((item) => {
    item.classList.toggle('active-fire', item.dataset.id === activeReminderId);
  });
}

function characterLabel(id) {
  const meta = (catalog.characters || []).find((c) => c.id === id);
  return meta?.name || id;
}

function renderList() {
  reminderList.innerHTML = '';
  if (!reminders.length) {
    reminderList.innerHTML = '<div class="hint">No reminders yet. Click + Add.</div>';
    return;
  }

  for (const r of reminders) {
    const item = document.createElement('div');
    item.className = 'reminder-item' + (openIds.has(r.id) ? ' open' : '');
    if (r.id === activeReminderId) item.classList.add('active-fire');
    item.dataset.id = r.id;

    const { text, cls } = timerLabel(r);
    const thumb = (catalog.characters || []).find((c) => c.id === r.character)?.thumb;

    item.innerHTML = `
      <button type="button" class="reminder-summary" data-toggle="${r.id}" aria-expanded="${openIds.has(r.id)}">
        <div>
          <div class="reminder-title">${escapeHtml(r.name)}</div>
          <div class="reminder-sub">${escapeHtml(characterLabel(r.character))} · ${escapeHtml(r.action)}</div>
        </div>
        <div class="reminder-timer${cls ? ` ${cls}` : ''}" data-timer="${r.id}">${text}</div>
        <svg class="chevron" viewBox="0 0 20 20" fill="none" aria-hidden="true">
          <path d="M5 8l5 5 5-5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>
      <div class="reminder-panel">
        <div class="reminder-panel-meta">
          ${thumb ? `<img src="../assets/characters/${escapeHtml(thumb)}" alt="" style="width:36px;height:36px;border-radius:10px;object-fit:cover;vertical-align:middle;margin-right:8px"/>` : ''}
          ${escapeHtml(r.scheduleSummary || r.schedule_summary || '')}
          ${
            (r.nextFire || r.next_fire)
              ? ` · Next at ${escapeHtml(r.nextFire || r.next_fire)}`
              : isPaused && remainingOf(r) != null
                ? ` · Frozen at ${fmt(remainingOf(r))}`
                : ''
          }
        </div>
        <div class="reminder-actions">
          <button class="btn btn-secondary btn-sm" data-edit="${r.id}">Edit</button>
          <button class="btn btn-ghost btn-sm" data-test="${r.id}">Test</button>
          <button class="btn btn-danger btn-sm" data-del="${r.id}">Delete</button>
          <label class="toggle"><input type="checkbox" data-enable="${r.id}" ${r.enabled ? 'checked' : ''}/> On</label>
        </div>
      </div>
    `;
    reminderList.appendChild(item);
  }

  reminderList.querySelectorAll('[data-toggle]').forEach((b) => {
    b.addEventListener('click', () => {
      const id = b.dataset.toggle;
      const item = reminderList.querySelector(`.reminder-item[data-id="${id}"]`);
      if (!item) return;
      const open = item.classList.toggle('open');
      b.setAttribute('aria-expanded', open ? 'true' : 'false');
      if (open) openIds.add(id);
      else openIds.delete(id);
    });
  });
  reminderList.querySelectorAll('[data-edit]').forEach((b) => {
    b.addEventListener('click', () => {
      const r = reminders.find((x) => x.id === b.dataset.edit);
      openEditor(r);
    });
  });
  reminderList.querySelectorAll('[data-test]').forEach((b) => {
    b.addEventListener('click', () => invoke('test_reminder', { id: b.dataset.test }));
  });
  reminderList.querySelectorAll('[data-del]').forEach((b) => {
    b.addEventListener('click', async () => {
      if (!confirm('Delete this reminder?')) return;
      await invoke('delete_reminder', { id: b.dataset.del });
      await refreshStatus();
    });
  });
  reminderList.querySelectorAll('[data-enable]').forEach((b) => {
    b.addEventListener('change', async () => {
      await invoke('set_reminder_enabled', { id: b.dataset.enable, enabled: b.checked });
      await refreshStatus();
    });
  });
}

function openEditor(r) {
  formOpen = true;
  editorCard.classList.remove('hidden');
  editorError.textContent = '';
  document.getElementById('editor-title').textContent = r ? 'Edit reminder' : 'New reminder';
  document.getElementById('edit-id').value = r?.id || '';
  document.getElementById('edit-name').value = r?.name || '';
  document.getElementById('edit-message').value = r?.message || 'Did you remember to drink water?';
  document.getElementById('edit-board').value = r?.boardText || '';
  document.getElementById('edit-not-yet').value =
    r?.notYetMessage || 'Do it now!';

  fillCharacterPicker(r?.character);
  syncActions(r?.action);
  syncBoardField();

  const st = r?.schedule?.type || 'interval';
  document.getElementById('edit-schedule-type').value = st;
  if (st === 'interval') {
    document.getElementById('edit-minutes').value = r?.schedule?.minutes || 15;
  } else if (st === 'once') {
    document.getElementById('edit-date').value = r?.schedule?.date || '';
    document.getElementById('edit-once-time').value = r?.schedule?.time || '09:00';
  } else if (st === 'daily') {
    document.getElementById('edit-daily-time').value = r?.schedule?.time || '09:00';
  } else if (st === 'weekly') {
    document.getElementById('edit-weekly-time').value = r?.schedule?.time || '09:00';
    const days = new Set(r?.schedule?.days || []);
    document.querySelectorAll('#edit-days input').forEach((cb) => {
      cb.checked = days.has(parseInt(cb.value, 10));
    });
  }
  syncSchedulePanes();
  document.getElementById('msg-count').textContent =
    `${document.getElementById('edit-message').value.length}/80`;
  updateBoardCounter();
  editorCard.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
}

function closeEditor() {
  formOpen = false;
  editorCard.classList.add('hidden');
  editorError.textContent = '';
  renderList();
}

function fillCharacterPicker(selected) {
  const picker = document.getElementById('edit-character-picker');
  const hidden = document.getElementById('edit-character');
  const chars = catalog.characters || [];
  let sel = selected || chars[0]?.id || '';
  if (sel && !chars.some((c) => c.id === sel)) sel = chars[0]?.id || '';
  hidden.value = sel;
  picker.innerHTML = '';

  for (const c of chars) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'char-card' + (c.id === sel ? ' selected' : '');
    btn.setAttribute('role', 'option');
    btn.setAttribute('aria-selected', c.id === sel ? 'true' : 'false');
    btn.dataset.id = c.id;

    const img = document.createElement('img');
    img.alt = c.name || c.id;
    img.src = c.thumb
      ? `../assets/characters/${c.thumb}`
      : `../assets/characters/${c.id}/thumb.webp`;
    img.onerror = () => {
      img.style.display = 'none';
    };

    const label = document.createElement('span');
    label.className = 'char-name';
    label.textContent = c.name || c.id;

    btn.appendChild(img);
    btn.appendChild(label);
    btn.addEventListener('click', () => {
      hidden.value = c.id;
      picker.querySelectorAll('.char-card').forEach((el) => {
        const on = el.dataset.id === c.id;
        el.classList.toggle('selected', on);
        el.setAttribute('aria-selected', on ? 'true' : 'false');
      });
      syncActions();
    });
    picker.appendChild(btn);
  }
}

function syncActions(preferred) {
  const cid = document.getElementById('edit-character').value;
  const meta = (catalog.characters || []).find((c) => c.id === cid);
  const actions =
    meta?.actions?.length
      ? meta.actions
      : meta?.type === 'model'
        ? catalog.sharedActions || ['idle', 'drink', 'board']
        : ['drink'];
  const sel = document.getElementById('edit-action');
  const keep = preferred || sel.value;
  sel.innerHTML = '';
  for (const a of actions) {
    const opt = document.createElement('option');
    opt.value = a;
    opt.textContent = a;
    if (a === keep) opt.selected = true;
    sel.appendChild(opt);
  }
  if (!sel.value && sel.options.length) sel.selectedIndex = 0;
  syncBoardField();
}

function updateBoardCounter() {
  const input = document.getElementById('edit-board');
  const { max, label } = boardLimits(input.value);
  input.maxLength = max;
  if (input.value.length > max) input.value = input.value.slice(0, max);
  document.getElementById('board-count').textContent = `${input.value.length}/${label}`;
}

function syncBoardField() {
  const show = document.getElementById('edit-action').value === 'board';
  document.getElementById('board-field').classList.toggle('hidden', !show);
  if (show) updateBoardCounter();
}

function syncSchedulePanes() {
  const t = document.getElementById('edit-schedule-type').value;
  document.getElementById('sched-interval').classList.toggle('hidden', t !== 'interval');
  document.getElementById('sched-once').classList.toggle('hidden', t !== 'once');
  document.getElementById('sched-daily').classList.toggle('hidden', t !== 'daily');
  document.getElementById('sched-weekly').classList.toggle('hidden', t !== 'weekly');
}

function buildSchedule() {
  const t = document.getElementById('edit-schedule-type').value;
  if (t === 'interval') {
    return {
      type: 'interval',
      minutes: Math.max(1, Math.min(240, parseInt(document.getElementById('edit-minutes').value, 10) || 15)),
    };
  }
  if (t === 'once') {
    return {
      type: 'once',
      date: document.getElementById('edit-date').value,
      time: document.getElementById('edit-once-time').value,
    };
  }
  if (t === 'daily') {
    return { type: 'daily', time: document.getElementById('edit-daily-time').value };
  }
  const days = [...document.querySelectorAll('#edit-days input:checked')].map((c) =>
    parseInt(c.value, 10),
  );
  return {
    type: 'weekly',
    days,
    time: document.getElementById('edit-weekly-time').value,
  };
}

function validateBoardText(text) {
  const t = text.trim();
  if (!t) return 'Board text is required for the board action';
  if (isBoardLink(t)) {
    if (t.length > MAX_BOARD_LINK) return `Board link must be <= ${MAX_BOARD_LINK} characters`;
    return '';
  }
  if (t.length < MIN_BOARD_TEXT || t.length > MAX_BOARD_TEXT) {
    return `Board text must be ${MIN_BOARD_TEXT}–${MAX_BOARD_TEXT} characters, or a link`;
  }
  return '';
}

async function saveEditor() {
  editorError.textContent = '';
  const action = document.getElementById('edit-action').value;
  const boardRaw = document.getElementById('edit-board').value.trim();
  if (action === 'board') {
    const err = validateBoardText(boardRaw);
    if (err) {
      editorError.textContent = err;
      return;
    }
  }
  const reminder = {
    id: document.getElementById('edit-id').value || '',
    name: document.getElementById('edit-name').value.trim(),
    enabled: true,
    character: document.getElementById('edit-character').value,
    action,
    message: document.getElementById('edit-message').value.trim(),
    boardText: action === 'board' ? boardRaw : null,
    notYetMessage: document.getElementById('edit-not-yet').value.trim() || 'Do it now!',
    schedule: buildSchedule(),
  };
  const existing = reminders.find((r) => r.id === reminder.id);
  if (existing) reminder.enabled = existing.enabled;

  try {
    await invoke('save_reminder', { reminder });
    closeEditor();
    await refreshStatus();
  } catch (err) {
    editorError.textContent = String(err);
  }
}

function fmt(totalSecs) {
  const s = Math.max(0, Math.round(Number(totalSecs) || 0));
  const m = Math.floor(s / 60);
  const sec = s % 60;
  if (m >= 60) {
    const h = Math.floor(m / 60);
    const mm = m % 60;
    return `${h}:${String(mm).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
  }
  return `${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

init().catch(console.error);
