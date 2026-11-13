import { basename, fmtBytes, fmtDur, fmtFps } from '../lib/format';
import type { JobUI } from '../lib/types';
import { BitmapText } from './BitmapText';
import { PixelField } from './PixelField';
import { Btn, Kbd, MOD } from './ui';

const STAGE: Record<string, string> = {
  decode: 'Reading frames', sr: 'Upscaling', encode: 'Encoding', mux: 'Finishing',
};

export function RunStage({ job, preview, onCancel }: { job: JobUI; preview: boolean; onCancel: () => void }) {
  const pr = job.progress;
  const frac = pr?.total ? pr.frame / pr.total : 0;
  const pct = Math.min(99, Math.floor(frac * 100));
  const label = job.state === 'queued' ? 'Waiting in the queue' : preview ? 'Rendering the 3 s preview' : STAGE[pr?.stage ?? ''] ?? 'Starting';
  return (
    <div className="run">
      <PixelField mode="progress" progress={frac} />
      <div className="run-card">
        <div className="run-num">
          <BitmapText text={String(pct).padStart(2, '0')} h={9} bold />
          <span className="pct">%</span>
        </div>
        <p className="lead">{label}…</p>
        <p className="micro tnum">
          {pr
            ? `${pr.frame.toLocaleString()} / ${pr.total.toLocaleString()} frames · ${pr.fps.toFixed(1)} fps · ${pr.etaSec != null ? `${fmtDur(pr.etaSec)} left` : 'estimating'}`
            : '(Waiting for the engine)'}
        </p>
        <Btn variant="ghost" onClick={onCancel}>
          (Cancel) <Kbd>{MOD}.</Kbd>
        </Btn>
      </div>
    </div>
  );
}

export function DoneStage(p: { job: JobUI; onReveal: () => void; onOpen: () => void; onAnother: () => void }) {
  const j = p.job;
  return (
    <div className="run">
      <PixelField mode="idle" hot />
      <div className="run-card">
        <BitmapText text="DONE" h={9} bold />
        <p className="lead">{basename(j.out ?? '')}</p>
        <p className="micro tnum">
          {j.outW}×{j.outH} · {fmtFps(j.outFps)} fps
          {j.stats ? ` · ${fmtBytes(j.stats.outBytes)} · took ${fmtDur(j.stats.elapsedSec)}` : ''}
        </p>
        <div className="row">
          <Btn onClick={p.onReveal}>(Reveal)</Btn>
          <Btn variant="ghost" onClick={p.onOpen}>(Open)</Btn>
          <Btn variant="ghost" onClick={p.onAnother}>(Another)</Btn>
        </div>
      </div>
    </div>
  );
}
