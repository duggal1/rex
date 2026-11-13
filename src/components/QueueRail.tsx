import { basename } from '../lib/format';
import type { JobUI } from '../lib/types';
import { Meter } from './ui';

export function QueueRail({ jobs, onCancel }: { jobs: JobUI[]; onCancel: (id: string) => void }) {
  return (
    <aside className="rail">
      <h3 className="micro rail-h">(Queue)</h3>
      {jobs.map((j) => {
        const f = j.progress && j.progress.total ? j.progress.frame / j.progress.total : j.state === 'done' ? 1 : 0;
        const live = j.state === 'running' || j.state === 'queued';
        return (
          <div key={j.id} className="job">
            <div className="job-top">
              <span className="job-name">{basename(j.input)}</span>
              {live && (
                <button type="button" className="textbtn" aria-label="Cancel job" onClick={() => onCancel(j.id)}>×</button>
              )}
            </div>
            <div className="micro tnum">{j.outW}×{j.outH} · {j.state}</div>
            <Meter value={f} />
          </div>
        );
      })}
    </aside>
  );
}