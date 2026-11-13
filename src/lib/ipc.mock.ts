// Plain-browser mock so the UI runs (and can be screenshotted) without Tauri.
import type { Ipc } from './ipc';
import type { JobEv, JobView, ProbeInfo } from './types';

const listeners = new Set<(e: { id: string; ev: JobEv }) => void>();
const emit = (id: string, ev: JobEv) => {
  listeners.forEach((listener) => {
    listener({ id, ev });
  });
};
const jobs = new Map<string, JobView>();
const timers = new Map<string, number>();
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

const info: ProbeInfo = {
  path: '/mock/meme-clip.mp4', container: 'mov,mp4', durationSec: 34, width: 640, height: 480,
  displayWidth: 640, displayHeight: 480, fpsNum: 30000, fpsDen: 1001, fps: 29.97, vfr: false,
  codec: 'h264', pixFmt: 'yuv420p', interlaced: true, sarNum: 1, sarDen: 1, rotation: 0,
  colorSpace: '', matrix: 'bt601', hdr: false, hasAudio: true, audioCodecs: ['aac'],
  sizeBytes: 12_400_000, bitrate: 2_900_000,
  warnings: [
    { code: 'interlaced_detected', text: 'Interlaced source. Deinterlacing automatically.' },
    { code: 'sd_matrix', text: 'Untagged SD video. Assuming BT.601 and converting to BT.709.' },
  ],
};

const still = (label: string) =>
  'data:image/svg+xml;utf8,' +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="960"><defs><linearGradient id="g"><stop offset="0" stop-color="#2370E8"/><stop offset="1" stop-color="#F3ACF0"/></linearGradient></defs><rect width="1280" height="960" fill="url(#g)"/><text x="60" y="140" font-size="96" font-family="sans-serif" fill="#0A0A0A">${label}</text></svg>`,
  );

export const mock: Ipc = {
  caps: async () => ({
    os: 'mock', ffmpeg: true, ai: true,
    encoders: { share: 'hevc_mock', edit: 'prores_mock', compat: 'h264_mock' },
    maxEdge: { share: 7680, edit: 3840, compat: 3840 },
  }),
  probe: async () => {
    await wait(900);
    return info;
  },
  defaultOutput: async (p, prof, edge) => `${p.replace(/\.[^.]+$/, '')} upscaled ${edge}.${prof === 'edit' ? 'mov' : 'mp4'}`,
  estimate: async (s) => ({
    frames: 1020, bytes: 480_000_000, outW: s.upscale.longEdge, outH: Math.round((s.upscale.longEdge * 3) / 4),
    outFps: 29.97, aiModel: 'realesr-animevideov3', aiScale: 2, contentScale: s.upscale.longEdge / 640, warnings: [],
  }),
  bench: async () => {
    await wait(1200);
    return 6.5;
  },
  enqueue: async (spec) => {
    const id = Math.random().toString(36).slice(2);
    const out = spec.encode.out;
    const v: JobView = {
      id, state: 'queued', input: spec.input.path, out, preview: spec.preview,
      totalFrames: spec.preview ? 90 : 1020, outW: spec.upscale.longEdge,
      outH: Math.round((spec.upscale.longEdge * 3) / 4), outFps: 29.97, error: null, stats: null,
    };
    jobs.set(id, v);
    setTimeout(() => {
      v.state = 'running';
      emit(id, { t: 'started' });
      let f = 0;
      const iv = window.setInterval(() => {
        f += Math.ceil(v.totalFrames / 40);
        const stage = f < v.totalFrames * 0.3 ? 'decode' : f < v.totalFrames * 0.8 ? 'sr' : 'encode';
        emit(id, { t: 'progress', stage, frame: Math.min(f, v.totalFrames), total: v.totalFrames, fps: 6.5, etaSec: (v.totalFrames - f) / 6.5 });
        if (f >= v.totalFrames) {
          clearInterval(iv);
          timers.delete(id);
          v.state = 'done';
          emit(id, { t: 'done', out, stats: { elapsedSec: 12, outBytes: 480e6, frames: v.totalFrames } });
        }
      }, 200);
      timers.set(id, iv);
    }, 50);
    return v;
  },
  cancel: async (id) => {
    const t = timers.get(id);
    if (t) clearInterval(t);
    timers.delete(id);
    const v = jobs.get(id);
    if (v) v.state = 'cancelled';
    emit(id, { t: 'cancelled' });
  },
  listJobs: async () => [...jobs.values()],
  extractFrame: async (p) => still(p.includes('upscaled') ? 'RESULT' : 'SOURCE'),
  reveal: async () => {},
  openPath: async () => {},
  getPrefs: async () => ({}),
  setPrefs: async () => {},
  pickFile: async () => '/mock/meme-clip.mp4',
  pickFolder: async () => '/mock/out',
  onJobEvent: async (cb) => {
    listeners.add(cb);
    return () => listeners.delete(cb);
  },
  onFileDrop: async () => () => {},
  assetUrl: (p) => p,
};
