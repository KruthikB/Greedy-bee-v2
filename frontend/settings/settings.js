const { invoke } = window.__TAURI__.core;
const { listen }  = window.__TAURI__.event;

const statusBadge   = document.getElementById('status-badge');
const countdownLbl  = document.getElementById('countdown-label');
const intervalInput = document.getElementById('interval-input');
const applyBtn      = document.getElementById('apply-btn');
const resumeBtn     = document.getElementById('resume-btn');
const testBtn       = document.getElementById('test-btn');
const quitBtn       = document.getElementById('quit-btn');

let tickTimer = null;

async function init() {
  await listen('pause-changed', () => refreshStatus());
  await listen('interval-changed', e => { intervalInput.value = e.payload; });
  await listen('overlay-dismissed', () => refreshStatus());

  await refreshStatus();
  tickTimer = setInterval(refreshStatus, 1000);

  applyBtn.addEventListener('click', applyInterval);
  resumeBtn.addEventListener('click', () => invoke('resume_reminders'));
  testBtn.addEventListener('click', () => invoke('test_reminder'));
  quitBtn.addEventListener('click', () => invoke('quit_app'));

  // Pause duration buttons
  document.querySelectorAll('[data-pause]').forEach(btn => {
    btn.addEventListener('click', () => {
      const mins = parseInt(btn.dataset.pause, 10);
      invoke('pause_reminders', { durationMinutes: mins === 0 ? null : mins });
    });
  });
}

async function refreshStatus() {
  let s;
  try { s = await invoke('get_status'); } catch { return; }

  intervalInput.value = s.interval_minutes;

  if (s.is_paused) {
    statusBadge.textContent = '⏸  Paused';
    statusBadge.className = 'badge badge-paused';
    countdownLbl.textContent = s.remaining_pause_secs < 0
      ? 'Paused indefinitely — click Resume to restart'
      : `Resuming in  ${fmt(s.remaining_pause_secs)}`;
    resumeBtn.disabled = false;
  } else {
    statusBadge.textContent = '●  Running';
    statusBadge.className = 'badge badge-running';
    countdownLbl.textContent = `Next reminder in  ${fmt(s.remaining_reminder_secs)}`;
    resumeBtn.disabled = true;
  }
}

async function applyInterval() {
  const minutes = Math.max(1, Math.min(240, parseInt(intervalInput.value, 10) || 15));
  intervalInput.value = minutes;
  await invoke('set_interval', { minutes });
  const orig = applyBtn.textContent;
  applyBtn.textContent = '✓ Applied';
  setTimeout(() => { applyBtn.textContent = orig; }, 1500);
}

function fmt(totalSecs) {
  const s = Math.max(0, Math.round(totalSecs));
  const m = Math.floor(s / 60);
  const sec = s % 60;
  return `${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
}

init().catch(console.error);
