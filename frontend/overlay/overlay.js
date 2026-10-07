// Tauri 2.x global IPC — available because withGlobalTauri: true
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const video       = document.getElementById('character-video');
const canvas      = document.getElementById('character-canvas');
const ctx         = canvas.getContext('2d', { alpha: true });
const strip       = document.getElementById('button-strip');
const questionLbl = document.getElementById('question-label');
const drinkNowLbl = document.getElementById('drink-now-label');
const yesBtn      = document.getElementById('yes-btn');
const noBtn       = document.getElementById('no-btn');
const doneBtn     = document.getElementById('done-btn');

let state = 'hidden'; // 'hidden' | 'playing' | 'asking' | 'drink-now'
let paintHandle = 0;
let safetyTimer = 0;
let usingVideoFrameCallback = typeof video.requestVideoFrameCallback === 'function';

function stopPainting() {
  if (usingVideoFrameCallback && paintHandle) {
    try { video.cancelVideoFrameCallback(paintHandle); } catch (_) { /* ignore */ }
  } else {
    cancelAnimationFrame(paintHandle);
  }
  paintHandle = 0;
}

function clearSafetyTimer() {
  if (safetyTimer) {
    clearTimeout(safetyTimer);
    safetyTimer = 0;
  }
}

function drawFrame() {
  if (video.videoWidth && video.videoHeight) {
    if (canvas.width !== video.videoWidth || canvas.height !== video.videoHeight) {
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
    }
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(video, 0, 0);
  }
}

function paintLoop() {
  drawFrame();
  if (video.paused || video.ended || state !== 'playing') {
    paintHandle = 0;
    return;
  }
  if (usingVideoFrameCallback) {
    paintHandle = video.requestVideoFrameCallback(() => paintLoop());
  } else {
    paintHandle = requestAnimationFrame(paintLoop);
  }
}

function startPainting() {
  stopPainting();
  paintLoop();
}

async function init() {
  await listen('reminder-fire', () => showReminder());
  video.addEventListener('play', startPainting);
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
  ctx.clearRect(0, 0, canvas.width, canvas.height);

  await invoke('set_overlay_clickthrough', { enabled: true });
  document.body.classList.remove('hidden');

  clearSafetyTimer();
  safetyTimer = setTimeout(() => {
    if (state === 'playing') onVideoEnded();
  }, 8000);

  video.currentTime = 0;
  try {
    await video.play();
  } catch (_) {
    onVideoEnded();
  }
}

async function onVideoEnded() {
  if (state !== 'playing') return;
  state = 'asking';
  clearSafetyTimer();
  stopPainting();
  drawFrame();

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

  clearSafetyTimer();
  stopPainting();
  state = 'hidden';
  strip.classList.remove('visible');
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  video.pause();
  video.currentTime = 0;

  await invoke('dismiss_overlay');
}

init().catch(console.error);
