import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { ipc } from './lib/ipc';
import { useJobs } from './hooks';
import { DEFAULT_RECIPE, SIZES } from './lib/types';
import type { AppErr, Caps, Estimate, JobSpec, JobUI, ProbeInfo, Profile, Recipe, Warning } from './lib/types';
import { TitleBar } from './components/TitleBar';
import { DropZone } from './components/DropZone';
import { ProbeStrip } from './components/ProbeStrip';
import { CompareViewer } from './components/CompareViewer';
import { RunStage, DoneStage } from './components/Stage';
import { RecipePanel } from './components/RecipePanel';
import { QueueRail } from './components/QueueRail';
import { PixelField } from './components/PixelField';
import { Banner } from './components/ui';

type Busy = 'none' | 'preview' | 'run';
type Notice = { err: AppErr; retry?: () => void };

const MAC = typeof navigator !== 'undefined' && /Mac/.test(navigator.userAgent);
const PROFILES: Profile[] = ['share', 'compat', 'edit'];
const WARN_TEXT: Record<string, string> = {
  ai_unavailable: 'AI engine or models not found. Using Lanczos resize only.',
};

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const edgeOf = (id: string) => SIZES.find((s) => s.id === id)?.edge ?? 3840;
const isLive = (j?: JobUI) => !!j && (j.state === 'queued' || j.state === 'running');

const toErr = (e: unknown): AppErr => {
  if (
    typeof e === 'object' &&
    e !== null &&
    'code' in e &&
    'msg' in e &&
    'action' in e &&
    typeof e.code === 'string' &&
    typeof e.msg === 'string' &&
    typeof e.action === 'string'
  ) {
    return { code: e.code, msg: e.msg, action: e.action };
  }
  return { code: 'E_INTERNAL', msg: String(e), action: 'Retry.' };
};

// The still used for compare, and the 3 s preview window around it.
const stillT = (f: ProbeInfo) => clamp(f.durationSec * 0.25, 0, Math.max(0, f.durationSec - 0.2));
const previewStart = (f: ProbeInfo) => clamp(stillT(f) - 1.5, 0, Math.max(0, f.durationSec - 3));

function buildSpec(r: Recipe, f: ProbeInfo, out: string, o: { preview: boolean; start: number }): JobSpec {
  return {
    v: 1,
    input: { path: f.path, range: { startSec: o.start, endSec: null } },
    upscale: { mode: r.mode, maxAiScale: r.mode === 'clean' ? 4 : 2, longEdge: edgeOf(r.size) },
    smooth: { mode: r.smooth, fps: 60 },
    frame: { canvas: r.canvas, fill: r.fill },
    encode: { profile: r.profile, quality: r.quality, out },
    preview: o.preview,
  };
}

function grabPair(outPath: string, tOut: number, srcPath: string, tSrc: number, w: number, h: number) {
  return Promise.all([
    ipc.extractFrame(outPath, tOut, null),
    ipc.extractFrame(srcPath, tSrc, [w, h]),
  ]).then(([r, s]) => ({ res: ipc.assetUrl(r), src: ipc.assetUrl(s) }));
}

export default function App() {
  const { jobs, upsert } = useJobs();

  const [caps, setCaps] = useState<Caps | null>(null);
  const [recipe, setRecipeState] = useState<Recipe>(DEFAULT_RECIPE);
  const [dir, setDir] = useState<string | null>(null);
  const [motion, setMotion] = useState(
    () => !(typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches),
  );
  const [ready, setReady] = useState(false);

  const [file, setFile] = useState<ProbeInfo | null>(null);
  const [probing, setProbing] = useState(false);
  const [hot, setHot] = useState(false);
  const [out, setOut] = useState('');
  const [outTick, setOutTick] = useState(0);

  const [est, setEst] = useState<Estimate | null>(null);
  const [estErr, setEstErr] = useState<AppErr | null>(null);
  const [bench, setBench] = useState<number | null | 'pending'>(null);

  const [probeErr, setProbeErr] = useState<AppErr | null>(null);
  const [startErr, setStartErr] = useState<AppErr | null>(null);

  const [srcPlain, setSrcPlain] = useState<string>();
  const [srcFit, setSrcFit] = useState<string>();
  const [res, setRes] = useState<string>();

  const [runId, setRunId] = useState<string | null>(null);
  const [prevId, setPrevId] = useState<string | null>(null);
  const [showCompare, setShowCompare] = useState(false);

  const lastPath = useRef<string | null>(null);

  const runJob = runId ? jobs[runId] : undefined;
  const prevJob = prevId ? jobs[prevId] : undefined;
  const busy: Busy = isLive(runJob) ? 'run' : isLive(prevJob) ? 'preview' : 'none';
  const active = busy === 'run' ? runJob : busy === 'preview' ? prevJob : undefined;

  const canRun = !!file && !!est && !estErr && !!out && !!caps?.ffmpeg;

  // ───────── boot: caps + prefs ─────────
  useEffect(() => {
    let dead = false;
    ipc.caps().then((c) => !dead && setCaps(c)).catch((e) => !dead && setStartErr(toErr(e)));
    ipc
      .getPrefs()
      .then((p) => {
        if (dead) return;
        if (p.recipe && typeof p.recipe === 'object') setRecipeState({ ...DEFAULT_RECIPE, ...p.recipe });
        if (typeof p.dir === 'string') setDir(p.dir);
        if (typeof p.motion === 'boolean') setMotion(p.motion);
      })
      .catch(() => {})
      .finally(() => !dead && setReady(true));
    return () => {
      dead = true;
    };
  }, []);

  useEffect(() => {
    if (!ready) return;
    ipc.setPrefs({ recipe, dir, motion }).catch(() => {});
  }, [ready, recipe, dir, motion]);

  useEffect(() => {
    document.documentElement.dataset.motion = motion ? 'on' : 'off';
  }, [motion]);

  // Clamp recipe to what this machine can actually encode.
  useEffect(() => {
    if (!caps) return;
    const ok = (p: Profile) => !!caps.encoders[p] && caps.maxEdge[p] > 0;
    let r = recipe;
    if (!ok(r.profile)) {
      const alt = PROFILES.find(ok);
      if (alt) r = { ...r, profile: alt };
    }
    const max = caps.maxEdge[r.profile];
    if (max > 0 && edgeOf(r.size) > max) {
      const fit = [...SIZES].reverse().find((s) => s.edge <= max);
      if (fit) r = { ...r, size: fit.id };
    }
    if (r !== recipe) setRecipeState(r);
  }, [caps, recipe]);

  const changeRecipe = (r: Recipe) => {
    setRecipeState(r);
    setRes(undefined);
    setSrcFit(undefined);
    setPrevId(null);
    setShowCompare(false);
  };

  // ───────── load a file ─────────
  const load = useCallback(async (path: string) => {
    lastPath.current = path;
    setProbing(true);
    setProbeErr(null);
    setStartErr(null);
    try {
      const info = await ipc.probe(path);
      setFile(info);
      setEst(null);
      setEstErr(null);
      setBench(null);
      setSrcPlain(undefined);
      setSrcFit(undefined);
      setRes(undefined);
      setRunId(null);
      setPrevId(null);
      setShowCompare(false);
      ipc
        .extractFrame(path, stillT(info), null)
        .then((p) => setSrcPlain(ipc.assetUrl(p)))
        .catch((e) => setProbeErr(toErr(e)));
    } catch (e) {
      setProbeErr(toErr(e));
    } finally {
      setProbing(false);
    }
  }, []);

  const pick = useCallback(async () => {
    const p = await ipc.pickFile();
    if (p) void load(p);
  }, [load]);

  const pickDir = async () => {
    const d = await ipc.pickFolder();
    if (d) setDir(d);
  };

  useEffect(() => {
    let un: (() => void) | undefined;
    let dead = false;
    ipc
      .onFileDrop((e) => {
        setHot(e.hot);
        if (e.path) void load(e.path);
      })
      .then((u) => (dead ? u() : (un = u)));
    return () => {
      dead = true;
      un?.();
    };
  }, [load]);

  // ───────── output name ─────────
  useEffect(() => {
    if (!file) {
      setOut('');
      return;
    }
    let dead = false;
    ipc
      .defaultOutput(file.path, recipe.profile, edgeOf(recipe.size), dir)
      .then((p) => !dead && setOut(p))
      .catch(() => {});
    return () => {
      dead = true;
    };
  }, [file, recipe.profile, recipe.size, dir, outTick]);

  // ───────── estimate (debounced) + bench ─────────
  const specBase = useMemo(
    () => (file && out ? buildSpec(recipe, file, out, { preview: false, start: 0 }) : null),
    [recipe, file, out],
  );

  useEffect(() => {
    if (!specBase) {
      setEst(null);
      setEstErr(null);
      return;
    }
    let dead = false;
    const t = setTimeout(() => {
      ipc
        .estimate(specBase)
        .then((e) => {
          if (dead) return;
          setEst(e);
          setEstErr(null);
        })
        .catch((e) => {
          if (dead) return;
          setEst(null);
          setEstErr(toErr(e));
        });
    }, 200);
    return () => {
      dead = true;
      clearTimeout(t);
    };
  }, [specBase]);

  const hasEst = !!est;
  useEffect(() => {
    if (!specBase || !hasEst || busy !== 'none') return;
    let dead = false;
    setBench('pending');
    const t = setTimeout(() => {
      ipc
        .bench(specBase)
        .then((v) => !dead && setBench(v))
        .catch(() => !dead && setBench(null));
    }, 900);
    return () => {
      dead = true;
      clearTimeout(t);
    };
  }, [specBase, hasEst, busy]);

  // ───────── run / preview / cancel ─────────
  const start = async (preview: boolean) => {
    if (!file || !out || !canRun || busy !== 'none') return;
    setStartErr(null);
    try {
      const spec = buildSpec(recipe, file, out, { preview, start: preview ? previewStart(file) : 0 });
      const v = await ipc.enqueue(spec);
      upsert(v);
      if (preview) {
        setPrevId(v.id);
        setRes(undefined);
        setSrcFit(undefined);
      } else {
        setRunId(v.id);
        setShowCompare(false);
      }
    } catch (e) {
      setStartErr(toErr(e));
    }
  };

  const cancelActive = () => {
    if (active) void ipc.cancel(active.id);
  };

  const another = () => {
    setFile(null);
    setRunId(null);
    setPrevId(null);
    setSrcPlain(undefined);
    setSrcFit(undefined);
    setRes(undefined);
    setEst(null);
    setEstErr(null);
    setProbeErr(null);
    setStartErr(null);
    setShowCompare(false);
    setBench(null);
  };

  // preview finished → pull a result still + a source still padded to the same canvas
  useEffect(() => {
    if (!prevJob || prevJob.state !== 'done' || !prevJob.out || !file) return;
    let dead = false;
    const s0 = previewStart(file);
    const rel = clamp(stillT(file) - s0, 0, Math.max(0, Math.min(3, file.durationSec - s0) - 0.2));
    grabPair(prevJob.out, rel, file.path, s0 + rel, prevJob.outW, prevJob.outH)
      .then((p) => {
        if (dead) return;
        setRes(p.res);
        setSrcFit(p.src);
      })
      .catch((e) => !dead && setStartErr(toErr(e)));
    return () => {
      dead = true;
    };
  }, [prevJob?.state, prevJob?.id]);

  // full run finished → same stills, and rotate the default output name
  useEffect(() => {
    if (!runJob || runJob.state !== 'done' || !runJob.out || !file) return;
    let dead = false;
    setOutTick((n) => n + 1);
    const t = stillT(file);
    grabPair(runJob.out, t, file.path, t, runJob.outW, runJob.outH)
      .then((p) => {
        if (dead) return;
        setRes(p.res);
        setSrcFit(p.src);
      })
      .catch(() => {});
    return () => {
      dead = true;
    };
  }, [runJob?.state, runJob?.id]);

  useEffect(() => {
    if (runJob?.state === 'cancelled') setRunId(null);
    if (prevJob?.state === 'cancelled') setPrevId(null);
  }, [runJob?.state, prevJob?.state]);

  // ───────── shortcuts ─────────
  const act = useRef({ pick: () => {}, run: () => {}, cancel: () => {} });
  act.current = { pick: () => void pick(), run: () => void start(false), cancel: cancelActive };

  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      const k = e.key.toLowerCase();
      if (k === 'o') {
        e.preventDefault();
        act.current.pick();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        act.current.run();
      } else if (e.key === '.') {
        e.preventDefault();
        act.current.cancel();
      }
    };
    window.addEventListener('keydown', on);
    return () => window.removeEventListener('keydown', on);
  }, []);

  // ───────── derived view state ─────────
  const railJobs = useMemo(
    () =>
      Object.values(jobs)
        .filter((j) => !j.preview)
        .reverse()
        .slice(0, 8),
    [jobs],
  );
  const liveCount = railJobs.filter(isLive).length;

  const warns = useMemo<Warning[]>(() => {
    if (!file) return [];
    const m = new Map<string, Warning>();
    file.warnings.forEach((w) => m.set(w.code, w));
    est?.warnings.forEach((c) => m.set(c, { code: c, text: WARN_TEXT[c] ?? 'Heads up.' }));
    (runJob?.warns ?? prevJob?.warns ?? []).forEach((w) => m.set(w.code, w));
    return [...m.values()];
  }, [file, est, runJob?.warns, prevJob?.warns]);

  const noFfmpeg: AppErr | null =
    caps && !caps.ffmpeg
      ? { code: 'E_NO_FFMPEG', msg: 'ffmpeg was not found.', action: 'Reinstall the app, or put ffmpeg and ffprobe on your PATH.' }
      : null;

  const notice: Notice | null = (() => {
    const retryPath = lastPath.current;
    const c: { err: AppErr | null | undefined; retry?: () => void }[] = [
      { err: probeErr, retry: retryPath ? () => void load(retryPath) : undefined },
      { err: startErr },
      { err: runJob?.state === 'failed' ? runJob.error : null, retry: () => void start(false) },
      { err: prevJob?.state === 'failed' ? prevJob.error : null, retry: () => void start(true) },
      { err: estErr },
      { err: noFfmpeg },
    ];
    const hit = c.find((x) => x.err);
    return hit && hit.err ? { err: hit.err, retry: hit.retry } : null;
  })();

  const src = res ? srcFit ?? srcPlain : srcPlain;

  let stage: ReactNode;
  if (!file) {
    stage = <DropZone probing={probing} hot={hot} onPick={() => void pick()} />;
  } else if (busy !== 'none' && active) {
    stage = <RunStage job={active} preview={busy === 'preview'} onCancel={cancelActive} />;
  } else if (runJob?.state === 'done' && !showCompare) {
    stage = (
      <DoneStage
        job={runJob}
        onReveal={() => void ipc.reveal(runJob.out ?? '')}
        onOpen={() => void ipc.openPath(runJob.out ?? '')}
        onAnother={another}
      />
    );
  } else if (!src && !probeErr) {
    stage = (
      <div className="stage-loading">
        <PixelField mode="loading" />
      </div>
    );
  } else {
    stage = <CompareViewer src={src} res={res} />;
  }

  return (
    <div className="app">
      <TitleBar mac={MAC} activeJobs={liveCount} motion={motion} onMotion={() => setMotion((m) => !m)} />
      <div className={`body${railJobs.length ? ' has-rail' : ''}`}>
        {railJobs.length > 0 && <QueueRail jobs={railJobs} onCancel={(id) => void ipc.cancel(id)} />}

        <div className="center">
          <main className="stage">
            {stage}
            {notice && (
              <div className="notice">
                <Banner error={notice.err} onRetry={notice.retry} />
              </div>
            )}
            {file && busy === 'none' && runJob?.state === 'done' && res && (
              <button type="button" className="textbtn micro stage-tools" onClick={() => setShowCompare((v) => !v)}>
                {showCompare ? '(Back to result)' : '(Compare with source)'}
              </button>
            )}
            {file && busy === 'none' && !res && src && runJob?.state !== 'done' && (
              <p className="micro stage-hint">(Preview 3 s to compare against the source)</p>
            )}
          </main>
          {file && <ProbeStrip info={file} warns={warns} />}
        </div>

        <RecipePanel
          recipe={recipe}
          setRecipe={changeRecipe}
          caps={caps}
          file={file}
          est={est}
          bench={bench}
          out={out}
          onPickDir={() => void pickDir()}
          busy={busy}
          progress={active?.progress}
          canRun={canRun && busy === 'none'}
          onRun={() => void start(false)}
          onPreview={() => void start(true)}
          onCancel={cancelActive}
        />
      </div>
    </div>
  );
}
