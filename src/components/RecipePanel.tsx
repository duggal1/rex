import { basename, fmtDur, fmtBytes, fmtFps } from '../lib/format';
import { SIZES } from '../lib/types';
import type { Caps, Estimate, ProbeInfo, Progress, Recipe } from '../lib/types';
import { PixelField } from './PixelField';
import { Btn, Field, Kbd, Meter, MOD, Segmented } from './ui';

interface Props {
  recipe: Recipe;
  setRecipe: (r: Recipe) => void;
  caps: Caps | null;
  file: ProbeInfo | null;
  est: Estimate | null;
  bench: number | null | 'pending';
  out: string;
  onPickDir: () => void;
  busy: 'none' | 'preview' | 'run';
  progress?: Progress;
  canRun: boolean;
  onRun: () => void;
  onPreview: () => void;
  onCancel: () => void;
}

export function RecipePanel(p: Props) {
  const { recipe: r, caps, est, file } = p;
  const set = <K extends keyof Recipe>(k: K, v: Recipe[K]) => p.setRecipe({ ...r, [k]: v });
  const max = caps ? caps.maxEdge[r.profile] : 0;
  const profOk = (x: 'share' | 'edit' | 'compat') => !caps || (!!caps.encoders[x] && caps.maxEdge[x] > 0);

  const eta =
    p.bench === 'pending' ? 'measuring…' : typeof p.bench === 'number' && est ? fmtDur(est.frames / p.bench) : '—';
  const honesty =
    file && est
      ? est.contentScale > 1.05
        ? `${file.displayWidth}×${file.displayHeight} → ${est.outW}×${est.outH} is ${est.contentScale.toFixed(1)}×. Expect smoother, not new detail.`
        : 'No upscaling needed. This just resizes.'
      : null;
  const locked = p.busy !== 'none';
  const pr = p.progress;
  const frac = pr && pr.total ? pr.frame / pr.total : 0;

  return (
    <aside className={`recipe${locked ? ' locked' : ''}`}>
      <Field label="Size" note={honesty}>
        <div className="seg wrap" role="radiogroup" aria-label="Size">
          {SIZES.map((s) => (
            <button
              key={s.id}
              type="button"
              role="radio"
              className="chip"
              aria-checked={r.size === s.id}
              aria-disabled={(!!caps && s.edge > max) || undefined}
              disabled={!!caps && s.edge > max}
              title={caps && s.edge > max ? `This Mac or PC can't encode ${s.label} with this export.` : undefined}
              onClick={() => !(caps && s.edge > max) && set('size', s.id)}
            >
              {s.label}
            </button>
          ))}
        </div>
      </Field>

      <Field label="Mode" note={r.mode === 'fast' ? 'AI 2×, then Lanczos to size.' : 'AI up to 4×. Slower, cleaner.'}>
        <Segmented label="Mode" value={r.mode} onChange={(v) => set('mode', v)}
          options={[{ value: 'fast', label: 'Fast' }, { value: 'clean', label: 'Clean' }]} />
      </Field>

      <Field label="Smoothness" note={r.smooth !== 'off' ? 'Interpolated frames also get upscaled, so this multiplies the time.' : undefined}>
        <Segmented label="Smoothness" value={r.smooth} onChange={(v) => set('smooth', v)}
          options={[{ value: 'off', label: 'Off' }, { value: 'x2', label: '×2' }, { value: 'fps', label: '60 fps' }]} />
      </Field>

      <Field label="Frame">
        <Segmented label="Canvas" value={r.canvas} onChange={(v) => set('canvas', v)}
          options={[{ value: 'original', label: 'Original' }, { value: '16:9', label: '16:9' }, { value: '9:16', label: '9:16' }, { value: '1:1', label: '1:1' }]} />
        {r.canvas !== 'original' && (
          <Segmented label="Fill" value={r.fill} onChange={(v) => set('fill', v)}
            options={[{ value: 'black', label: 'Black bars' }, { value: 'blur', label: 'Blurred copy' }]} />
        )}
      </Field>

      <Field label="Quality">
        <Segmented label="Quality" value={r.quality} onChange={(v) => set('quality', v)}
          options={[{ value: 'high', label: 'High' }, { value: 'balanced', label: 'Balanced' }]} />
      </Field>

      <Field
        label="Export"
        note={
          caps ? (
            <>
              {caps.encoders[r.profile] ?? 'no encoder'} · {p.out ? basename(p.out) : '—'}{' '}
              <button type="button" className="textbtn" onClick={p.onPickDir}>(Change folder)</button>
            </>
          ) : undefined
        }
      >
        <Segmented label="Export" value={r.profile} onChange={(v) => set('profile', v)}
          options={[
            { value: 'share', label: 'Share · HEVC', disabled: !profOk('share') },
            { value: 'edit', label: 'Edit · ProRes', disabled: !profOk('edit') },
            { value: 'compat', label: 'Compatible · H.264', disabled: !profOk('compat') },
          ]} />
        {!caps && (
          <div className="mini-load">
            <PixelField mode="loading" cell={6} />
          </div>
        )}
      </Field>

      <div className="spacer" />

      <section className="est">
        {est ? (
          <>
            <p className="tnum">~{eta} · {fmtBytes(est.bytes)}</p>
            <p className="micro">{est.aiModel ? `AI ${est.aiScale}× · ${est.aiModel}` : 'Lanczos only'}</p>
          </>
        ) : (
          <p className="micro">{file ? '(Calculating…)' : '(Load a video to see an estimate)'}</p>
        )}
      </section>

      {p.busy === 'none' ? (
        <div className="runbar">
          <Btn disabled={!p.canRun} onClick={p.onRun}>
            {est ? `Upscale → ${est.outW}×${est.outH} · ${fmtFps(est.outFps)} fps` : 'Upscale'}
          </Btn>
          <Btn variant="ghost" disabled={!p.canRun} onClick={p.onPreview}>(Preview 3 s)</Btn>
          <p className="micro keys"><Kbd>{MOD}↵</Kbd> run · <Kbd>\</Kbd> A/B · <Kbd>Z</Kbd> zoom</p>
        </div>
      ) : (
        <div className="runbar">
          <Meter value={frac} />
          <p className="micro tnum">
            {p.busy === 'preview' ? 'Preview' : 'Upscaling'} {pr ? `${pr.frame.toLocaleString()} / ${pr.total.toLocaleString()}` : ''}
          </p>
          <Btn variant="ghost" onClick={p.onCancel}>(Cancel)</Btn>
        </div>
      )}
    </aside>
  );
}
