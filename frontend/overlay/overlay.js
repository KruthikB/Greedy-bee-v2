// Tauri 2.x global IPC — available because withGlobalTauri: true
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const video       = document.getElementById('character-video');
const strip       = document.getElementById('button-strip');
const questionLbl = document.getElementById('question-label');
const drinkNowLbl = document.getElementById('drink-now-label');
const yesBtn      = document.getElementById('yes-btn');
const noBtn       = document.getElementById('no-btn');
const doneBtn     = document.getElementById('done-btn');

let state = 'hidden'; // 'hidden' | 'playing' | 'asking' | 'drink-now'

async function init() {
  await listen('reminder-fire', () => showReminder());
  video.addEventListener('ended', onVideoEnded);
  video.addEventListener('error', onVideoEnded);
  yesBtn.addEventListener('click', dismissOverlay);
  doneBtn.addEventListener('click', dismissOverlay);
  noBtn.addEventListener('click', onNo);
}

async function showReminder() {
  if (state !== 'hidden') return;
  state = 'playing';

  strip.classList.remove('visible');
  questionLbl.style.display = 'block';
  drinkNowLbl.style.display = 'none';
  yesBtn.style.display = 'block';
  noBtn.style.display = 'block';
  doneBtn.style.display = 'none';

  await invoke('set_overlay_clickthrough', { enabled: true });
  document.body.classList.remove('hidden');

  video.currentTime = 0;
  try { await video.play(); } catch (_) { /* autoplay policy — rare on desktop */ }
}

async function onVideoEnded() {
  if (state !== 'playing') return;
  state = 'asking';

  await invoke('set_overlay_clickthrough', { enabled: false });
  strip.classList.add('visible');
}

function onNo() {
  state = 'drink-now';
  questionLbl.style.display = 'none';
  yesBtn.style.display = 'none';
  noBtn.style.display = 'none';
  drinkNowLbl.style.display = 'block';
  doneBtn.style.display = 'block';
}

async function dismissOverlay() {
  document.body.classList.add('hidden');
  await new Promise(r => setTimeout(r, 350));

  state = 'hidden';
  strip.classList.remove('visible');
  video.pause();
  video.currentTime = 0;

  await invoke('dismiss_overlay');
}

init().catch(console.error);
