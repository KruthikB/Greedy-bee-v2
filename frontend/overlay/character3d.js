/**
 * Lightweight three.js character player for model-type packs.
 * Uses procedural drink/board/wave/talk/idle actions so no external
 * animation GLBs are required. Renders into a transparent WebGL canvas
 * when possible; falls back to copying frames into a 2D canvas.
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { MeshoptDecoder } from 'three/addons/libs/meshopt_decoder.module.js';

// Portrait frame matching the video character slot (left of screen, 300px tall).
const WIDTH = 168;
const HEIGHT = 300;

let renderer = null;
let scene = null;
let camera = null;
let mixer = null;
let clock = null;
let raf = 0;
let currentRoot = null;
let boardMesh = null;
let cupMesh = null;
// Always render offscreen and blit into the page's 2D canvas.
// A canvas cannot have both 2D and WebGL contexts, and transparent
// WebView2 already paints 2D canvases reliably.
let fallback2d = null;
let meshoptReady = null;

const loader = new GLTFLoader();

const modelCache = new Map();

async function ensureMeshopt() {
  if (!meshoptReady) {
    meshoptReady = (async () => {
      if (MeshoptDecoder.ready) await MeshoptDecoder.ready;
      loader.setMeshoptDecoder(MeshoptDecoder);
    })();
  }
  await meshoptReady;
}

function findBone(root, names) {
  let found = null;
  root.traverse((obj) => {
    if (found || !obj.isBone && obj.type !== 'Bone' && !obj.name) return;
    const n = (obj.name || '').toLowerCase();
    for (const want of names) {
      if (n === want.toLowerCase() || n.endsWith(want.toLowerCase()) || n.includes(want.toLowerCase())) {
        found = obj;
        return;
      }
    }
  });
  // Also check Object3D nodes (our starter uses Object3D "bones")
  if (!found) {
    root.traverse((obj) => {
      if (found) return;
      const n = (obj.name || '').toLowerCase();
      for (const want of names) {
        if (n === want.toLowerCase() || n.endsWith(want.toLowerCase())) {
          found = obj;
          return;
        }
      }
    });
  }
  return found;
}

function makeBoardTexture(text) {
  const c = document.createElement('canvas');
  c.width = 512;
  c.height = 256;
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#f5f0e6';
  ctx.fillRect(0, 0, c.width, c.height);
  ctx.strokeStyle = '#8b7355';
  ctx.lineWidth = 10;
  ctx.strokeRect(8, 8, c.width - 16, c.height - 16);
  ctx.fillStyle = '#1a1a1a';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  const msg = (text || '').trim().slice(0, 30) || '…';
  let size = 48;
  ctx.font = `bold ${size}px Segoe UI, Arial, sans-serif`;
  while (size > 18 && ctx.measureText(msg).width > c.width - 40) {
    size -= 2;
    ctx.font = `bold ${size}px Segoe UI, Arial, sans-serif`;
  }
  // Word wrap
  const words = msg.split(/\s+/);
  const lines = [];
  let line = '';
  for (const w of words) {
    const test = line ? `${line} ${w}` : w;
    if (ctx.measureText(test).width > c.width - 40 && line) {
      lines.push(line);
      line = w;
    } else {
      line = test;
    }
  }
  if (line) lines.push(line);
  const lineH = size + 6;
  const startY = c.height / 2 - ((lines.length - 1) * lineH) / 2;
  lines.forEach((l, i) => ctx.fillText(l, c.width / 2, startY + i * lineH));
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.needsUpdate = true;
  return tex;
}

function attachProps(root, action, boardText) {
  clearProps();
  const handR = findBone(root, ['Hand_R', 'RightHand', 'mixamorigRightHand', 'hand_r', 'r_hand']);
  const spine = findBone(root, ['Spine', 'Spine2', 'Chest', 'mixamorigSpine', 'spine']);
  const parent = handR || spine || root;

  if (action === 'board') {
    const geo = new THREE.PlaneGeometry(0.45, 0.28);
    const mat = new THREE.MeshBasicMaterial({
      map: makeBoardTexture(boardText),
      side: THREE.DoubleSide,
      transparent: false,
    });
    boardMesh = new THREE.Mesh(geo, mat);
    boardMesh.position.set(0, 0.15, 0.22);
    (spine || parent).add(boardMesh);
  } else if (action === 'drink') {
    const geo = new THREE.CylinderGeometry(0.04, 0.035, 0.12, 12);
    const mat = new THREE.MeshStandardMaterial({ color: 0x4fc3f7, roughness: 0.3, metalness: 0.1 });
    cupMesh = new THREE.Mesh(geo, mat);
    cupMesh.position.set(0.05, -0.05, 0.08);
    parent.add(cupMesh);
  }
}

function clearProps() {
  if (boardMesh) {
    boardMesh.parent?.remove(boardMesh);
    boardMesh.geometry?.dispose();
    boardMesh.material?.map?.dispose();
    boardMesh.material?.dispose();
    boardMesh = null;
  }
  if (cupMesh) {
    cupMesh.parent?.remove(cupMesh);
    cupMesh.geometry?.dispose();
    cupMesh.material?.dispose();
    cupMesh = null;
  }
}

/** Simple procedural clip: rotate RightArm / Hand for drink/wave/board. */
function buildProceduralClip(root, action, duration = 6) {
  const times = [0, duration * 0.25, duration * 0.5, duration * 0.75, duration];
  const tracks = [];

  const armR = findBone(root, ['RightArm', 'mixamorigRightArm', 'upperarm_r', 'RightUpperArm']);
  const armL = findBone(root, ['LeftArm', 'mixamorigLeftArm', 'upperarm_l', 'LeftUpperArm']);
  const handR = findBone(root, ['Hand_R', 'RightHand', 'mixamorigRightHand']);

  const q = (x, y, z) => {
    const e = new THREE.Euler(x, y, z);
    const qq = new THREE.Quaternion().setFromEuler(e);
    return [qq.x, qq.y, qq.z, qq.w];
  };

  if (action === 'drink' && armR) {
    const values = [
      ...q(0, 0, 0),
      ...q(-1.2, 0, -0.3),
      ...q(-1.6, 0.2, -0.2),
      ...q(-1.2, 0, -0.3),
      ...q(0, 0, 0),
    ];
    tracks.push(new THREE.QuaternionKeyframeTrack(`${armR.name}.quaternion`, times, values));
  } else if (action === 'wave' && armR) {
    const values = [
      ...q(0, 0, 0),
      ...q(0, 0, -2.2),
      ...q(0, 0, -1.6),
      ...q(0, 0, -2.2),
      ...q(0, 0, 0),
    ];
    tracks.push(new THREE.QuaternionKeyframeTrack(`${armR.name}.quaternion`, times, values));
  } else if (action === 'board' && armR && armL) {
    const raise = [
      ...q(0, 0, 0),
      ...q(-0.9, 0.2, -0.4),
      ...q(-0.9, 0.2, -0.4),
      ...q(-0.9, 0.2, -0.4),
      ...q(0, 0, 0),
    ];
    const raiseL = [
      ...q(0, 0, 0),
      ...q(-0.9, -0.2, 0.4),
      ...q(-0.9, -0.2, 0.4),
      ...q(-0.9, -0.2, 0.4),
      ...q(0, 0, 0),
    ];
    tracks.push(new THREE.QuaternionKeyframeTrack(`${armR.name}.quaternion`, times, raise));
    tracks.push(new THREE.QuaternionKeyframeTrack(`${armL.name}.quaternion`, times, raiseL));
  } else if (action === 'talk' && (handR || armR)) {
    const target = armR || handR;
    const values = [
      ...q(0, 0, 0),
      ...q(-0.3, 0.1, 0),
      ...q(-0.15, -0.1, 0),
      ...q(-0.35, 0.05, 0),
      ...q(0, 0, 0),
    ];
    tracks.push(new THREE.QuaternionKeyframeTrack(`${target.name}.quaternion`, times, values));
  } else {
    // idle: slight sway on root
    const rootNode = root;
    const values = [
      ...q(0, 0, 0),
      ...q(0, 0.08, 0),
      ...q(0, -0.08, 0),
      ...q(0, 0.05, 0),
      ...q(0, 0, 0),
    ];
    tracks.push(new THREE.QuaternionKeyframeTrack(`${rootNode.name || 'Root'}.quaternion`, times, values));
  }

  return new THREE.AnimationClip(action, duration, tracks);
}

function ensureRenderer(targetCanvas) {
  if (renderer) {
    fallback2d = targetCanvas.getContext('2d', { alpha: true });
    targetCanvas.width = WIDTH;
    targetCanvas.height = HEIGHT;
    targetCanvas.style.width = `${WIDTH}px`;
    targetCanvas.style.height = `${HEIGHT}px`;
    return;
  }
  clock = new THREE.Clock();
  scene = new THREE.Scene();
  // Narrow FOV portrait framing — full body, feet near bottom.
  camera = new THREE.PerspectiveCamera(32, WIDTH / HEIGHT, 0.05, 100);

  const hemi = new THREE.HemisphereLight(0xffffff, 0x444466, 1.2);
  scene.add(hemi);
  const dir = new THREE.DirectionalLight(0xffffff, 1.0);
  dir.position.set(2, 4, 3);
  scene.add(dir);

  renderer = new THREE.WebGLRenderer({
    alpha: true,
    antialias: true,
    preserveDrawingBuffer: true,
    premultipliedAlpha: false,
  });
  renderer.setClearColor(0x000000, 0);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.setSize(WIDTH, HEIGHT, false);
  fallback2d = targetCanvas.getContext('2d', { alpha: true });
  targetCanvas.width = WIDTH;
  targetCanvas.height = HEIGHT;
  targetCanvas.style.width = `${WIDTH}px`;
  targetCanvas.style.height = `${HEIGHT}px`;
}

async function loadModel(characterId) {
  await ensureMeshopt();
  if (modelCache.has(characterId)) {
    return modelCache.get(characterId).clone(true);
  }
  const url = `../assets/characters/${characterId}/model.glb`;
  const gltf = await loader.loadAsync(url);
  modelCache.set(characterId, gltf.scene);
  return gltf.scene.clone(true);
}

function frameModel(root) {
  // Reset then fit full body into the portrait frame (feet on ground).
  root.position.set(0, 0, 0);
  root.rotation.set(0, 0, 0);
  root.scale.set(1, 1, 1);
  root.updateMatrixWorld(true);

  const box = new THREE.Box3().setFromObject(root);
  const size = box.getSize(new THREE.Vector3());
  const height = Math.max(size.y, 0.001);
  const scale = 1.7 / height;
  root.scale.setScalar(scale);
  root.updateMatrixWorld(true);

  const box2 = new THREE.Box3().setFromObject(root);
  const center = box2.getCenter(new THREE.Vector3());
  root.position.x -= center.x;
  root.position.z -= center.z;
  root.position.y -= box2.min.y;
  root.updateMatrixWorld(true);

  const box3 = new THREE.Box3().setFromObject(root);
  const h = Math.max(box3.max.y - box3.min.y, 0.001);
  // Camera looks at mid-body so head and feet stay in frame.
  const midY = h * 0.48;
  camera.position.set(0, midY, h * 2.15);
  camera.lookAt(0, midY, 0);
  camera.updateProjectionMatrix();
}

function stopLoop() {
  if (raf) {
    cancelAnimationFrame(raf);
    raf = 0;
  }
}

function disposeSceneContent() {
  clearProps();
  if (currentRoot) {
    scene?.remove(currentRoot);
    currentRoot.traverse((obj) => {
      if (obj.geometry) obj.geometry.dispose();
      if (obj.material) {
        const mats = Array.isArray(obj.material) ? obj.material : [obj.material];
        mats.forEach((m) => {
          m.map?.dispose();
          m.dispose();
        });
      }
    });
    currentRoot = null;
  }
  mixer = null;
}

/**
 * Play a model character action once. Resolves when finished (or on error).
 * @param {HTMLCanvasElement} canvas
 * @param {{ character: string, action: string, boardText?: string }} opts
 */
export async function playModelCharacter(canvas, opts) {
  const { character, action = 'idle', boardText } = opts;
  ensureRenderer(canvas);
  stopLoop();
  disposeSceneContent();

  let root;
  try {
    root = await loadModel(character);
  } catch (err) {
    console.error('Failed to load model', character, err);
    throw err;
  }

  scene.add(root);
  currentRoot = root;
  frameModel(root);
  attachProps(root, action, boardText);

  const clip = buildProceduralClip(root, action, action === 'board' ? 7 : 6);
  mixer = new THREE.AnimationMixer(root);
  const clipAction = mixer.clipAction(clip);
  clipAction.setLoop(THREE.LoopOnce, 1);
  clipAction.clampWhenFinished = true;
  clipAction.play();

  return new Promise((resolve) => {
    let finished = false;
    const onFinished = () => {
      if (finished) return;
      finished = true;
      mixer?.removeEventListener('finished', onFinished);
      stopLoop();
      resolve();
    };
    mixer.addEventListener('finished', onFinished);

    const safety = setTimeout(onFinished, (clip.duration + 0.5) * 1000);

    const tick = () => {
      raf = requestAnimationFrame(tick);
      const dt = clock.getDelta();
      mixer?.update(dt);
      renderer.render(scene, camera);
      if (fallback2d) {
        fallback2d.clearRect(0, 0, WIDTH, HEIGHT);
        fallback2d.drawImage(renderer.domElement, 0, 0, WIDTH, HEIGHT);
      }
    };
    clock.start();
    tick();

    // Clear safety when finished normally
    const orig = onFinished;
    mixer.addEventListener('finished', () => clearTimeout(safety));
    void orig;
  });
}

/** Kept for API compatibility — offscreen blit is always used. */
export function forceOffscreenFallback() {}

export function disposeCharacter3d() {
  stopLoop();
  disposeSceneContent();
  if (renderer) {
    renderer.dispose();
    renderer = null;
  }
  scene = null;
  camera = null;
  clock = null;
  fallback2d = null;
}
