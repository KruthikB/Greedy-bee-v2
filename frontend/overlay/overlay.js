// Tauri 2.x global IPC — available because withGlobalTauri: true
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const video      = document.getElementById('character-video');
const canvas     = document.getElementById('character-canvas');
const ctx        = canvas.getContext('2d', { alpha: true });
const strip      = document.getElementById('button-strip');
const questionLbl = document.getElementById('question-label');
const drinkNowLbl = document.getElementById('drink-now-label');
const yesBtn     = document.getElementById('yes-btn');
const noBtn      = document.getElementById('no-btn');
const doneBtn    = document.getElementById('done-btn');

let state = 'hidden'; // 'hidden' | 'playing' | 'asking' | 'drink-now'
let paintFrame = 0;

function drawCharacter() {
  if (video.videoWidth && video.videoHeight) {
    if (canvas.width !== video.videoWidth || canvas.height !== video.videoHeight) {
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
    }
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(video, 0, 0);
  }
  if (!video.paused && !video.ended) {
    paintFrame = requestAnimationFrame(drawCharacter);
  }
}

async function init() {
  await listen('reminder-fire', () => showReminder());
  video.addEventListener('play', () => {
    cancelAnimationFrame(paintFrame);
    drawCharacter();
  });
  video.addEventListener('ended', onVideoEnded);
  video.addEventListener('error', onVideoEnded);
  yesBtn.addEventListener('click', dismissOverlay);
  doneBtn.addEventListener('click', dismissOverlay);
  noBtn.addEventListener('click', onNo);
}

async function showReminder() {
  if (state !== 'hidden') return;
  state = 'playing';

  // Reset UI
  strip.classList.remove('visible');
  questionLbl.style.display = 'block';
  drinkNowLbl.style.display = 'none';
  yesBtn.style.display = 'block';
  noBtn.style.display = 'block';
  doneBtn.style.display = 'none';

  // Enable click-through while video plays (decorative, not interactive)
  await invoke('set_overlay_clickthrough', { enabled: true });

  // Fade in via CSS transition on <body>
  document.body.classList.remove('hidden');

  // Start video from the beginning
  video.currentTime = 0;
  try { await video.play(); } catch (_) { /* autoplay policy — rare on desktop */ }
}

async function onVideoEnded() {
  if (state !== 'playing') return;
  state = 'asking';

  // Disable click-through so buttons are clickable
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
  // Fade out
  document.body.classList.add('hidden');

  // Wait for CSS transition (300ms + small buffer)
  await new Promise(r => setTimeout(r, 350));

  state = 'hidden';
  strip.classList.remove('visible');
  cancelAnimationFrame(paintFrame);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  video.pause();
  video.currentTime = 0;

  // Tell Rust to hide the window + reset scheduler countdown
  await invoke('dismiss_overlay');
}

init().catch(console.error);
