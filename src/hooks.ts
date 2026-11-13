import { useCallback, useEffect, useState } from 'react';
import { ipc } from './lib/ipc';
import type { JobUI, JobView } from './lib/types';

export function useJobs() {
  const [jobs, setJobs] = useState<Record<string, JobUI>>({});

  const upsert = useCallback((v: JobView) => {
    setJobs((j) => ({ ...j, [v.id]: { ...(j[v.id] ?? { warns: [] }), ...v } }));
  }, []);

  useEffect(() => {
    let un: (() => void) | undefined;
    let dead = false;
    ipc
      .onJobEvent(({ id, ev }) => {
        setJobs((j) => {
          const cur = j[id];
          if (!cur) {
            // event beat the enqueue response: resync from the backend
            ipc.listJobs().then((vs) => vs.forEach(upsert));
            return j;
          }
          switch (ev.t) {
            case 'started': return { ...j, [id]: { ...cur, state: 'running' } };
            case 'progress': return { ...j, [id]: { ...cur, progress: ev } };
            case 'warn': return { ...j, [id]: { ...cur, warns: [...cur.warns, { code: ev.code, text: ev.text }] } };
            case 'done': return { ...j, [id]: { ...cur, state: 'done', out: ev.out, stats: ev.stats } };
            case 'error': return { ...j, [id]: { ...cur, state: 'failed', error: ev.error } };
            case 'cancelled': return { ...j, [id]: { ...cur, state: 'cancelled' } };
          }
        });
      })
      .then((unlisten) => {
        if (dead) {
          unlisten();
          return;
        }
        un = unlisten;
      });
    return () => {
      dead = true;
      un?.();
    };
  }, [upsert]);

  return { jobs, upsert };
}
