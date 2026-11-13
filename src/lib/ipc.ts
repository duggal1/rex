import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { mock } from './ipc.mock';
import type { AppErr, Caps, Estimate, JobEv, JobSpec, JobView, ProbeInfo, Profile } from './types';

export interface Ipc {
  caps: () => Promise<Caps>;
  probe: (path: string) => Promise<ProbeInfo>;
  defaultOutput: (path: string, profile: Profile, longEdge: number, dir?: string | null) => Promise<string>;
  estimate: (spec: JobSpec) => Promise<Estimate>;
  bench: (spec: JobSpec) => Promise<number | null>;
  enqueue: (spec: JobSpec) => Promise<JobView>;
  cancel: (id: string) => Promise<void>;
  listJobs: () => Promise<JobView[]>;
  extractFrame: (path: string, time: number, fit?: [number, number] | null) => Promise<string>;
  reveal: (path: string) => Promise<void>;
  openPath: (path: string) => Promise<void>;
  getPrefs: () => Promise<Record<string, unknown>>;
  setPrefs: (prefs: Record<string, unknown>) => Promise<void>;
  pickFile: () => Promise<string | null>;
  pickFolder: () => Promise<string | null>;
  onJobEvent: (callback: (event: { id: string; ev: JobEv }) => void) => Promise<() => void>;
  onFileDrop: (callback: (event: { hot: boolean; path: string | null }) => void) => Promise<() => void>;
  assetUrl: (path: string) => string;
}

const tauri: Ipc = {
  caps: () => invoke<Caps>('get_caps'),
  probe: (path) => invoke<ProbeInfo>('probe_file', { path }),
  defaultOutput: (path, profile, longEdge, dir) =>
    invoke<string>('default_output', { path, profile, longEdge, dir }),
  estimate: (spec) => invoke<Estimate>('estimate', { spec }),
  bench: (spec) => invoke<number | null>('bench', { spec }),
  enqueue: (spec) => invoke<JobView>('enqueue', { spec }),
  cancel: (id) => invoke<void>('cancel_job', { id }),
  listJobs: () => invoke<JobView[]>('list_jobs'),
  extractFrame: (path, time, fit) => invoke<string>('extract_frame', { path, t: time, fit }),
  reveal: (path) => invoke<void>('reveal', { path }),
  openPath: (path) => invoke<void>('open_path', { path }),
  getPrefs: () => invoke<Record<string, unknown>>('get_prefs'),
  setPrefs: (prefs) => invoke<void>('set_prefs', { prefs }),
  async pickFile() {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Video', extensions: ['mp4', 'mov', 'm4v', 'mkv', 'webm', 'avi'] }],
    });
    return typeof selected === 'string' ? selected : null;
  },
  async pickFolder() {
    const selected = await open({ directory: true, multiple: false });
    return typeof selected === 'string' ? selected : null;
  },
  onJobEvent: (callback) => listen<{ id: string; ev: JobEv }>('job-event', (event) => callback(event.payload)),
  onFileDrop: (callback) =>
    getCurrentWindow().onDragDropEvent(({ payload }) => {
      switch (payload.type) {
        case 'enter':
        case 'over':
          callback({ hot: true, path: null });
          break;
        case 'drop':
          callback({ hot: false, path: payload.paths[0] ?? null });
          break;
        case 'leave':
          callback({ hot: false, path: null });
          break;
      }
    }),
  assetUrl: (path) => convertFileSrc(path),
};

function getError(error: unknown): AppErr {
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'msg' in error &&
    'action' in error &&
    typeof error.code === 'string' &&
    typeof error.msg === 'string' &&
    typeof error.action === 'string'
  ) {
    return { code: error.code, msg: error.msg, action: error.action };
  }
  return {
    code: 'E_INTERNAL',
    msg: error instanceof Error ? error.message : String(error),
    action: 'Retry. If it keeps happening, copy the details and report it.',
  };
}

export { getError };

const isTauri = typeof window !== 'undefined' && Reflect.has(window, '__TAURI_INTERNALS__');
export const ipc: Ipc = isTauri ? tauri : mock;
