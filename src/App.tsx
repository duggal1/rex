import { useCallback, useEffect, useMemo, useState } from 'react';
import { CompareViewer } from './components/CompareViewer';
import { DropZone } from './components/DropZone';
import { ProbeStrip } from './components/ProbeStrip';
import { QueueRail } from './components/QueueRail';
import { RecipePanel } from './components/RecipePanel';
import { DoneStage, RunStage } from './components/Stage';
import { TitleBar } from './components/TitleBar';
import { Banner } from './components/ui';
import { useJobs } from './hooks';
import { getError, ipc } from './lib/ipc';
import type { AppErr, JobSpec, ProbeInfo, Recipe } from './lib/types';
import { DEFAULT_RECIPE, SIZES } from './lib/types';

function makeSpec(file: ProbeInfo, recipe: Recipe, output: string, preview: boolean): JobSpec {
  const size = SIZES.find((candidate) => candidate.id === recipe.size) ?? SIZES[2];
  return {
    v: 1,
    input: { path: file.path, range: { startSec: 0, endSec: null } },
    upscale: { mode: recipe.mode, maxAiScale: recipe.mode === 'clean' ? 4 : 2, longEdge: size.edge },
    smooth: { mode: recipe.smooth, fps: 60 },
    frame: { canvas: recipe.canvas, fill: recipe.fill },
    encode: { profile: recipe.profile, quality: recipe.quality, out: output || file.path },
    preview,
  };
}

export default function App() {
  const [caps, setCaps] = useState<Awaited<ReturnType<typeof ipc.caps>> | null>(null);
  const [file, setFile] = useState<ProbeInfo | null>(null);
  const [recipe, setRecipe] = useState<Recipe>(DEFAULT_RECIPE);
  const [output, setOutput] = useState('');
  const [outputFolder, setOutputFolder] = useState<string | undefined>();
  const [estimate, setEstimate] = useState<Awaited<ReturnType<typeof ipc.estimate>> | null>(null);
  const [sourceImage, setSourceImage] = useState<string | undefined>();
  const [resultImage, setResultImage] = useState<string | undefined>();
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [error, setError] = useState<AppErr | null>(null);
  const [probing, setProbing] = useState(false);
  const [motion, setMotion] = useState(false);
  const [bench, setBench] = useState<number | null | 'pending'>(null);
  const { jobs, upsert } = useJobs();

  const activeJob = activeJobId ? jobs[activeJobId] : undefined;
  const jobList = useMemo(() => Object.values(jobs), [jobs]);
  const busy = activeJob?.state === 'queued' || activeJob?.state === 'running';
  const appBusy = busy ? (activeJob.preview ? 'preview' : 'run') : 'none';
  const activeCount = jobList.filter((job) => job.state === 'queued' || job.state === 'running').length;

  const loadFile = useCallback(async (path: string) => {
    setProbing(true);
    setError(null);
    setFile(null);
    setEstimate(null);
    setOutput('');
    setSourceImage(undefined);
    setResultImage(undefined);
    setActiveJobId(null);
    try {
      const nextFile = await ipc.probe(path);
      setFile(nextFile);
      try {
        const imagePath = await ipc.extractFrame(path, Math.min(nextFile.durationSec / 2, 2));
        setSourceImage(ipc.assetUrl(imagePath));
      } catch (frameError: unknown) {
        setError(getError(frameError));
      }
    } catch (probeError: unknown) {
      setError(getError(probeError));
    } finally {
      setProbing(false);
    }
  }, []);

  const pickFile = useCallback(async () => {
    try {
      const path = await ipc.pickFile();
      if (path) await loadFile(path);
    } catch (pickError: unknown) {
      setError(getError(pickError));
    }
  }, [loadFile]);

  useEffect(() => {
    let disposed = false;
    let unlistenDrop: (() => void) | undefined;
    ipc.caps()
      .then((value) => {
        if (!disposed) setCaps(value);
      })
      .catch((capsError: unknown) => {
        if (!disposed) setError(getError(capsError));
      });
    ipc.onFileDrop(loadFile).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenDrop = unlisten;
    });
    return () => {
      disposed = true;
      unlistenDrop?.();
    };
  }, [loadFile]);

  useEffect(() => {
    if (!file) return;
    const size = SIZES.find((candidate) => candidate.id === recipe.size) ?? SIZES[2];
    let cancelled = false;
    ipc.defaultOutput(file.path, recipe.profile, size.edge, outputFolder)
      .then((value) => {
        if (!cancelled) setOutput(value);
      })
      .catch((outputError: unknown) => {
        if (!cancelled) setError(getError(outputError));
      });
    return () => {
      cancelled = true;
    };
  }, [file, outputFolder, recipe.profile, recipe.size]);

  useEffect(() => {
    if (!file || !output) return;
    let cancelled = false;
    const spec = makeSpec(file, recipe, output, false);
    ipc.estimate(spec)
      .then((value) => {
        if (!cancelled) setEstimate(value);
      })
      .catch((estimateError: unknown) => {
        if (!cancelled) setError(getError(estimateError));
      });
    return () => {
      cancelled = true;
    };
  }, [file, output, recipe]);

  useEffect(() => {
    if (!file || !output) {
      setBench(null);
      return;
    }
    let cancelled = false;
    setBench('pending');
    ipc.bench(makeSpec(file, recipe, output, false))
      .then((rate) => {
        if (!cancelled) setBench(rate);
      })
      .catch(() => {
        if (!cancelled) setBench(null);
      });
    return () => {
      cancelled = true;
    };
  }, [file, output, recipe]);

  useEffect(() => {
    if (activeJob?.state !== 'done' || !activeJob.out) return;
    let cancelled = false;
    ipc.extractFrame(activeJob.out, 1.5, [activeJob.outW, activeJob.outH])
      .then((imagePath) => {
        if (!cancelled) setResultImage(ipc.assetUrl(imagePath));
      })
      .catch((frameError: unknown) => {
        if (!cancelled) setError(getError(frameError));
      });
    return () => {
      cancelled = true;
    };
  }, [activeJob?.out, activeJob?.outH, activeJob?.outW, activeJob?.state]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      if (target instanceof HTMLElement && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return;
      const modifier = event.metaKey || event.ctrlKey;
      if (modifier && event.key.toLowerCase() === 'o') {
        event.preventDefault();
        void pickFile();
      }
      if (modifier && event.key === 'Enter' && file && output && !busy) {
        event.preventDefault();
        void startJob(false);
      }
      if (modifier && event.key === '.' && activeJob && busy) {
        event.preventDefault();
        void ipc.cancel(activeJob.id);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  });

  const startJob = useCallback(async (preview: boolean) => {
    if (!file || !output) return;
    setError(null);
    try {
      const job = await ipc.enqueue(makeSpec(file, recipe, output, preview));
      upsert(job);
      setActiveJobId(job.id);
      setResultImage(undefined);
    } catch (enqueueError: unknown) {
      setError(getError(enqueueError));
    }
  }, [file, output, recipe, upsert]);

  const cancelActive = useCallback(() => {
    if (activeJob) void ipc.cancel(activeJob.id);
  }, [activeJob]);

  const pickOutputFolder = useCallback(async () => {
    try {
      const folder = await ipc.pickFolder();
      if (folder) setOutputFolder(folder);
    } catch (folderError: unknown) {
      setError(getError(folderError));
    }
  }, []);

  const revealOutput = useCallback(() => {
    if (activeJob?.out) void ipc.reveal(activeJob.out).catch((revealError: unknown) => setError(getError(revealError)));
  }, [activeJob]);

  const openOutput = useCallback(() => {
    if (activeJob?.out) void ipc.openPath(activeJob.out).catch((openError: unknown) => setError(getError(openError)));
  }, [activeJob]);

  const handleRecipeChange = useCallback((nextRecipe: Recipe) => {
    setRecipe(nextRecipe);
    setBench(null);
  }, []);

  const canRun = Boolean(file && output && !busy && (!caps || caps.ffmpeg));
  const recipeEstimate = estimate ?? null;
  const displayedWarnings = file?.warnings ?? [];

  return (
    <main className={`app${motion ? ' motion' : ''}`}>
      <TitleBar mac={caps?.os === 'macos'} activeJobs={activeCount} motion={motion} onMotion={() => setMotion((value) => !value)} />
      {error && <Banner error={error} onRetry={file ? () => void loadFile(file.path) : undefined} />}
      <div className="workspace">
        <section className="work-area" aria-label="Video workspace">
          {!file ? (
            <DropZone probing={probing} hot={false} onPick={() => void pickFile()} />
          ) : busy && activeJob ? (
            <RunStage job={activeJob} preview={activeJob.preview} onCancel={cancelActive} />
          ) : activeJob?.state === 'done' ? (
            <DoneStage job={activeJob} onReveal={revealOutput} onOpen={openOutput} onAnother={() => void pickFile()} />
          ) : (
            <CompareViewer src={sourceImage} res={resultImage} />
          )}
          {file && <ProbeStrip info={file} warns={displayedWarnings} />}
        </section>
        <RecipePanel
          recipe={recipe}
          setRecipe={handleRecipeChange}
          caps={caps}
          file={file}
          est={recipeEstimate}
          bench={bench}
          out={output}
          onPickDir={() => void pickOutputFolder()}
          busy={appBusy}
          progress={activeJob?.progress}
          canRun={canRun}
          onRun={() => void startJob(false)}
          onPreview={() => void startJob(true)}
          onCancel={cancelActive}
        />
        {jobList.length > 0 && <QueueRail jobs={jobList} onCancel={(id) => void ipc.cancel(id)} />}
      </div>
    </main>
  );
}
