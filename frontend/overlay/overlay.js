const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const canvas = document.getElementById('character-canvas');
const ctx = canvas.getContext('2d', { alpha: true });
const strip = document.getElementById('button-strip');
const questionLbl = document.getElementById('question-label');
const drinkNowLbl = document.getElementById('drink-now-label');
const yesBtn = document.getElementById('yes-btn');
const noBtn = document.getElementById('no-btn');
const doneBtn = document.getElementById('done-btn');

let state = 'hidden'; // 'hidden' | 'playing' | 'asking' | 'drink-now'
let activeReminder = null;
let videoCache = new Map(); // key: character/action -> { frames, fps, boardRect, loading }
let paintTimer = 0;
let safetyTimer = 0;
let catalog = null;
let character3d = null; // lazy-loaded only for model packs
let characterSize = 55; // percent of video slot height; aspect ratio preserved
let holdFrame = null; // last bitmap + board text to keep until ack

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

function applyCharacterSize(percent) {
  const p = Math.max(30, Math.min(100, Math.round(Number(percent) || 55)));
  characterSize = p;
  canvas.style.height = `${p}%`;
  canvas.style.maxHeight = `${p}%`;
  canvas.style.width = 'auto';
  canvas.style.maxWidth = 'min(720px, 70%)';
}

async function loadCatalog() {
  if (catalog) return catalog;
  try {
    catalog = await fetch('../assets/characters/catalog.json').then((r) => {
      if (!r.ok) throw new Error(`catalog ${r.status}`);
      return r.json();
    });
  } catch (err) {
    console.warn('catalog missing, using defaults', err);
    catalog = {
      characters: [
        { id: 'water-guy', name: 'Kaybie', type: 'video', actions: ['drink', 'board'] },
      ],
    };
  }
  return catalog;
}

function characterMeta(id) {
  const cat = catalog?.characters || [];
  return cat.find((c) => c.id === id) || { id, type: 'video', actions: ['drink'] };
}

async function ensureCharacter3d() {
  if (!character3d) {
    character3d = await import('./character3d.js');
  }
  return character3d;
}

function loadOneFrame(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error(`failed ${src}`));
    img.src = src;
  });
}

async function loadVideoFrames(character, action) {
  const key = `${character}/${action}`;
  if (videoCache.has(key)) return videoCache.get(key);

  const base = `../assets/characters/${character}/${action}`;
  const manifest = await fetch(`${base}/manifest.json`).then((r) => {
    if (!r.ok) throw new Error(`manifest ${r.status} for ${key}`);
    return r.json();
  });

  const fps = Number(manifest.fps) || 12;
  const count = Number(manifest.frameCount) || 0;
  const boardRect = manifest.boardRect || null;
  const frames = new Array(count);
  const prefetch = Math.min(8, count);
  await Promise.all(
    Array.from({ length: prefetch }, async (_, i) => {
      const n = String(i + 1).padStart(4, '0');
      frames[i] = await loadOneFrame(`${base}/frame_${n}.webp`);
    }),
  );
  const pack = { frames, fps, boardRect, base, count };
  videoCache.set(key, pack);
  void fillRemainingFrames(pack);
  return pack;
}

async function fillRemainingFrames(pack) {
  const { frames, base, count } = pack;
  for (let i = 0; i < count; i++) {
    if (frames[i]) continue;
    const n = String(i + 1).padStart(4, '0');
    try {
      frames[i] = await loadOneFrame(`${base}/frame_${n}.webp`);
    } catch (err) {
      console.warn(err);
    }
  }
}

async function ensureFrame(pack, index) {
  if (pack.frames[index]) return pack.frames[index];
  const n = String(index + 1).padStart(4, '0');
  const img = await loadOneFrame(`${pack.base}/frame_${n}.webp`);
  pack.frames[index] = img;
  return img;
}

function wrapLines(measureCtx, text, maxWidth) {
  const words = String(text || '').split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  if (words.length === 1) return [words[0]];
  const lines = [];
  let line = '';
  for (const w of words) {
    const test = line ? `${line} ${w}` : w;
    if (measureCtx.measureText(test).width > maxWidth && line) {
      lines.push(line);
      line = w;
    } else {
      line = test;
    }
  }
  if (line) lines.push(line);
  return lines;
}

function drawBoardText(text, boardRect, canvasW, canvasH) {
  if (!text || !boardRect) return;
  const x = boardRect.x * canvasW;
  const y = boardRect.y * canvasH;
  const w = boardRect.w * canvasW;
  const h = boardRect.h * canvasH;
  if (w < 4 || h < 4) return;

  const pad = Math.max(2, Math.min(w, h) * 0.06);
  const innerW = w - pad * 2;
  const innerH = h - pad * 2;
  const msg = String(text).trim();
  if (!msg) return;

  ctx.save();
  ctx.beginPath();
  ctx.rect(x, y, w, h);
  ctx.clip();

  ctx.fillStyle = '#1a1a1a';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';

  let size = Math.min(28, Math.floor(innerH * 0.42));
  let lines = [];
  while (size >= 8) {
    ctx.font = `bold ${size}px Segoe UI, Arial, sans-serif`;
    lines = wrapLines(ctx, msg, innerW);
    const lineH = size * 1.15;
    const blockH = lines.length * lineH;
    const widest = lines.reduce((m, ln) => Math.max(m, ctx.measureText(ln).width), 0);
    if (widest <= innerW && blockH <= innerH) break;
    size -= 1;
  }
  ctx.font = `bold ${size}px Segoe UI, Arial, sans-serif`;
  lines = wrapLines(ctx, msg, innerW);
  const lineH = size * 1.15;
  const blockH = lines.length * lineH;
  let cy = y + pad + (innerH - blockH) / 2 + lineH / 2;
  const cx = x + w / 2;
  for (const ln of lines) {
    ctx.fillText(ln, cx, cy, innerW);
    cy += lineH;
  }
  ctx.restore();
}

function paintHoldFrame() {
  if (!holdFrame) return;
  const { img, boardRect, boardText, W, H } = holdFrame;
  canvas.width = W;
  canvas.height = H;
  applyCharacterSize(characterSize);
  ctx.clearRect(0, 0, W, H);
  ctx.drawImage(img, 0, 0);
  if (boardText && boardRect) {
    drawBoardText(boardText, boardRect, W, H);
  }
}

function playVideoPack(pack, opts = {}) {
  const { boardRect = null, boardText = '' } = opts;
  const count = pack.count || pack.frames.length;
  return new Promise((resolve) => {
    if (!count) {
      resolve();
      return;
    }
    const first = pack.frames[0];
    const W = first.naturalWidth;
    const H = first.naturalHeight;
    canvas.width = W;
    canvas.height = H;
    applyCharacterSize(characterSize);
    const frameMs = Math.max(16, Math.round(1000 / pack.fps));
    const textStart = boardRect && boardText
      ? Math.max(0, Math.floor(count * 0.75))
      : count;
    let index = 0;

    const step = async () => {
      if (state !== 'playing') {
        resolve();
        return;
      }
      try {
        const img = await ensureFrame(pack, index);
        for (let j = index + 1; j < Math.min(count, index + 6); j++) {
          if (!pack.frames[j]) void ensureFrame(pack, j);
        }
        ctx.clearRect(0, 0, W, H);
        ctx.drawImage(img, 0, 0);
        const showText = index >= textStart;
        if (showText) {
          drawBoardText(boardText, boardRect, W, H);
        }
        holdFrame = {
          img,
          boardRect: showText ? boardRect : null,
          boardText: showText ? boardText : '',
          W,
          H,
        };
      } catch (err) {
        console.error('frame play error', err);
      }
      index += 1;
      if (index >= count) {
        // Keep the final pose + board text until the user answers.
        if (holdFrame && boardText && boardRect) {
          holdFrame.boardRect = boardRect;
          holdFrame.boardText = boardText;
          paintHoldFrame();
        }
        resolve();
        return;
      }
      paintTimer = setTimeout(step, frameMs);
    };
    step();
  });
}

async function init() {
  applyCharacterSize(characterSize);
  await listen('reminder-fire', (e) => showReminder(e.payload || {}));
  await listen('character-size-changed', (e) => {
    applyCharacterSize(e.payload);
    if (state !== 'hidden') paintHoldFrame();
  });
  yesBtn.addEventListener('click', dismissOverlay);
  doneBtn.addEventListener('click', dismissOverlay);
  noBtn.addEventListener('click', onNo);
  try {
    const status = await invoke('get_status');
    applyCharacterSize(status.characterSize ?? status.character_size ?? characterSize);
  } catch (_) {
    /* ignore */
  }
  try {
    await invoke('overlay_ready');
  } catch (err) {
    console.warn('overlay_ready', err);
  }
  void loadCatalog();
}

async function showReminder(payload) {
  // One character at a time — ignore fires while another is still on screen.
  if (state !== 'hidden') return;
  state = 'playing';
  if (!catalog) await loadCatalog();

  applyCharacterSize(payload.characterSize ?? payload.character_size ?? characterSize);

  activeReminder = {
    id: payload.id || null,
    character: payload.character || 'water-guy',
    action: payload.action || 'drink',
    message: payload.message || 'Did you remember to drink water?',
    boardText: payload.boardText || payload.board_text || '',
    notYetMessage:
      payload.notYetMessage ||
      payload.not_yet_message ||
      'Drink Now! Get up and drink a glass of water.',
  };
  holdFrame = null;

  strip.classList.remove('visible');
  questionLbl.textContent = `💧  ${activeReminder.message}`;
  questionLbl.style.display = 'block';
  drinkNowLbl.textContent = `💧  ${activeReminder.notYetMessage}`;
  drinkNowLbl.style.display = 'none';
  yesBtn.style.display = 'block';
  noBtn.style.display = 'block';
  doneBtn.style.display = 'none';
  clearCanvas();
  clearTimers();
  if (character3d) {
    try {
      character3d.disposeCharacter3d();
    } catch (_) {
      /* ignore */
    }
  }

  await invoke('set_overlay_clickthrough', { enabled: true });
  document.body.classList.remove('hidden');

  // Only unblock the asking UI if playback stalls — never auto-dismiss.
  safetyTimer = setTimeout(() => {
    if (state === 'playing') onAnimationEnded();
  }, 20000);

  const meta = characterMeta(activeReminder.character);
  try {
    if (meta.type === 'model') {
      const mod = await ensureCharacter3d();
      try {
        await mod.playModelCharacter(canvas, {
          character: activeReminder.character,
          action: activeReminder.action,
          boardText: activeReminder.boardText,
        });
      } catch (err) {
        console.warn('WebGL path failed, retrying offscreen', err);
        mod.forceOffscreenFallback();
        await mod.playModelCharacter(canvas, {
          character: activeReminder.character,
          action: activeReminder.action,
          boardText: activeReminder.boardText,
        });
      }
    } else {
      let action = activeReminder.action;
      let pack;
      try {
        pack = await loadVideoFrames(activeReminder.character, action);
      } catch (err) {
        // Missing action pack (e.g. Kaybie has no distinct board clip yet).
        console.warn(`pack ${activeReminder.character}/${action} missing, falling back to drink`, err);
        action = 'drink';
        pack = await loadVideoFrames(activeReminder.character, action);
      }
      await playVideoPack(pack, {
        boardRect: action === 'board' ? pack.boardRect : null,
        boardText: action === 'board' ? activeReminder.boardText : '',
      });
    }
  } catch (err) {
    console.error('character play failed', err);
  }
  await onAnimationEnded();
}

async function onAnimationEnded() {
  if (state !== 'playing') return;
  state = 'asking';
  clearTimers();
  paintHoldFrame();
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
  paintHoldFrame();
}

async function dismissOverlay() {
  document.body.classList.add('hidden');
  await new Promise((r) => setTimeout(r, 350));

  clearTimers();
  state = 'hidden';
  strip.classList.remove('visible');
  holdFrame = null;
  clearCanvas();
  if (character3d) {
    try {
      character3d.disposeCharacter3d();
    } catch (_) {
      /* ignore */
    }
  }

  const id = activeReminder?.id || null;
  activeReminder = null;
  await invoke('dismiss_overlay', { id });
}

init().catch(console.error);
