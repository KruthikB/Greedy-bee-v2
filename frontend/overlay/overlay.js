// Tauri 2.x global IPC — available because withGlobalTauri: true
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const canvas      = document.getElementById('character-canvas');
const ctx         = canvas.getContext('2d', { alpha: true });
const container   = document.getElementById('video-container');
const strip       = document.getElementById('button-strip');
const questionLbl = document.getElementById('question-label');
const drinkNowLbl = document.getElementById('drink-now-label');
const yesBtn      = document.getElementById('yes-btn');
const noBtn       = document.getElementById('no-btn');
const doneBtn     = document.getElementById('done-btn');

let state = 'hidden'; // 'hidden' | 'playing' | 'asking' | 'drink-now'
let frames = [];
let fps = 12;
let framesReady = null;
let paintTimer = 0;
let safetyTimer = 0;

function clearTimers() {
  if (paintTimer) {
    clearTimeout(paintTimer);
    paintTimer = 0;
  }
  if (safetyTimer) {
    clearTimeout(safetyTimer);
    safetyTimer = 0;
  }
}

function clearCanvas() {
  ctx.clearRect(0, 0, canvas.width || 1, canvas.height || 1);
}

async function loadFrames() {
  const manifest = await fetch('../assets/frames/manifest.json').then((r) => {
    if (!r.ok) throw new Error(`manifest ${r.status}`);
    return r.json();
  });
  fps = Number(manifest.fps) || 12;
  const count = Number(manifest.frameCount) || 0;
  if (count < 1) throw new Error('no frames in manifest');

  const loaded = await Promise.all(
    Array.from({ length: count }, (_, i) => {
      const n = String(i + 1).padStart(4, '0');
      const img = new Image();
      img.src = `../assets/frames/frame_${n}.webp`;
      return new Promise((resolve, reject) => {
        img.onload = () => resolve(img);
        img.onerror = () => reject(new Error(`failed to load frame_${n}.webp`));
      });
    }),
  );
  frames = loaded;
  return frames;
}

function ensureFrames() {
  if (!framesReady) {
    framesReady = loadFrames().catch((err) => {
      console.error('Greedy Bee: character frames failed to load', err);
      framesReady = null;
      throw err;
    });
  }
  return framesReady;
}

function playFrames() {
  if (!frames.length) {
    onAnimationEnded();
    return;
  }

  // Use the widest frame so narrow entry-frames don't clip later ones.
  const maxW    = frames.reduce((m, f) => Math.max(m, f.naturalWidth), 0);
  const targetH = frames[0].naturalHeight;

  canvas.width  = maxW;
  canvas.height = targetH;
  // Explicit inline styles — don't rely on `width: auto` in WebView2.
  canvas.style.width  = maxW   + 'px';
  canvas.style.height = targetH + 'px';

  const frameMs = Math.max(16, Math.round(1000 / fps));
  let index = 0;

  const step = () => {
    if (state !== 'playing') return;
    const frame = frames[index];
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(frame, 0, 0);
    index += 1;
    if (index >= frames.length) {
      onAnimationEnded();
      return;
    }
    paintTimer = setTimeout(step, frameMs);
  };

  step();
}

async function init() {
  await listen('reminder-fire', () => showReminder());
  yesBtn.addEventListener('click', dismissOverlay);
  doneBtn.addEventListener('click', dismissOverlay);
  noBtn.addEventListener('click', onNo);
  // Warm the frame cache while settings is open.
  ensureFrames().catch(() => {});
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
  clearCanvas();
  clearTimers();

  await invoke('set_overlay_clickthrough', { enabled: true });
  document.body.classList.remove('hidden');

  safetyTimer = setTimeout(() => {
    if (state === 'playing') onAnimationEnded();
  }, 12000);

  try {
    await ensureFrames();
    playFrames();
  } catch (_) {
    onAnimationEnded();
  }
}

async function onAnimationEnded() {
  if (state !== 'playing') return;
  state = 'asking';
  clearTimers();

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
  await new Promise((r) => setTimeout(r, 350));

  clearTimers();
  state = 'hidden';
  strip.classList.remove('visible');
  clearCanvas();

  await invoke('dismiss_overlay');
}

init().catch(console.error);
