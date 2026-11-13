export type Mode = 'fast' | 'clean';
export type SmoothMode = 'off' | 'x2' | 'fps';
export type Canvas = 'original' | '16:9' | '9:16' | '1:1';
export type Fill = 'black' | 'blur';
export type Profile = 'share' | 'edit' | 'compat';
export type Quality = 'high' | 'balanced';

export interface Warning { code: string; text: string }

export interface ProbeInfo {
  path: string; container: string; durationSec: number;
  width: number; height: number; displayWidth: number; displayHeight: number;
  fpsNum: number; fpsDen: number; fps: number; vfr: boolean;
  codec: string; pixFmt: string; interlaced: boolean;
  sarNum: number; sarDen: number; rotation: number;
  colorSpace: string; matrix: string; hdr: boolean;
  hasAudio: boolean; audioCodecs: string[];
  sizeBytes: number; bitrate: number; warnings: Warning[];
}

export interface Caps {
  os: string; ffmpeg: boolean; ai: boolean;
  encoders: { share: string | null; edit: string | null; compat: string | null };
  maxEdge: { share: number; edit: number; compat: number };
}

export interface AppErr { code: string; msg: string; action: string }
export interface Stats { elapsedSec: number; outBytes: number; frames: number }
export type JobState = 'queued' | 'running' | 'done' | 'failed' | 'cancelled';

export interface JobView {
  id: string; state: JobState; input: string; out: string | null; preview: boolean;
  totalFrames: number; outW: number; outH: number; outFps: number;
  error: AppErr | null; stats: Stats | null;
}

export interface Progress { stage: string; frame: number; total: number; fps: number; etaSec: number | null }
export interface JobUI extends JobView { progress?: Progress; warns: Warning[] }

export type JobEv =
  | { t: 'started' }
  | ({ t: 'progress' } & Progress)
  | { t: 'warn'; code: string; text: string }
  | { t: 'done'; out: string; stats: Stats }
  | { t: 'error'; error: AppErr }
  | { t: 'cancelled' };

export interface Estimate {
  frames: number; bytes: number; outW: number; outH: number; outFps: number;
  aiModel: string | null; aiScale: number; contentScale: number; warnings: string[];
}

export interface JobSpec {
  v: 1;
  input: { path: string; range: { startSec: number; endSec: number | null } };
  upscale: { mode: Mode; maxAiScale: number; longEdge: number };
  smooth: { mode: SmoothMode; fps: number };
  frame: { canvas: Canvas; fill: Fill };
  encode: { profile: Profile; quality: Quality; out: string };
  preview: boolean;
}

export interface Recipe {
  size: string; mode: Mode; smooth: SmoothMode; canvas: Canvas; fill: Fill; profile: Profile; quality: Quality;
}

export const SIZES = [
  { id: '1080', label: '1080p', edge: 1920 },
  { id: '1440', label: '1440p', edge: 2560 },
  { id: '4k', label: '4K', edge: 3840 },
  { id: '5k', label: '5K', edge: 5120 },
  { id: '6k', label: '6K', edge: 6144 },
  { id: '7k', label: '7K', edge: 7168 },
  { id: '8k', label: '8K', edge: 7680 },
];

export const DEFAULT_RECIPE: Recipe = {
  size: '4k', mode: 'fast', smooth: 'off', canvas: 'original', fill: 'black', profile: 'share', quality: 'high',
};