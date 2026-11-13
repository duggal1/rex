import { fmtBytes, fmtClock, fmtFps } from '../lib/format';
import type { ProbeInfo, Warning } from '../lib/types';
import { Pill } from './ui';

const LABEL: Record<string, string> = {
  interlaced_detected: 'Interlaced', non_square_pixels: 'Non-square px', rotated: 'Rotated', vfr: 'VFR',
  sd_matrix: 'SD colors', hdr_unsupported: 'HDR', tiny_source: 'Tiny source', ai_unavailable: 'No AI engine',
};
const BAD = new Set(['hdr_unsupported', 'tiny_source', 'ai_unavailable']);

export function ProbeStrip({ info, warns }: { info: ProbeInfo; warns: Warning[] }) {
  return (
    <footer className="probe">
      <span className="tnum">{info.displayWidth}×{info.displayHeight}</span><i />
      <span className="tnum">{fmtFps(info.fps)} fps</span><i />
      <span>{info.codec.toUpperCase()}</span><i />
      <span className="tnum">{fmtClock(info.durationSec)}</span><i />
      <span className="tnum">{fmtBytes(info.sizeBytes)}</span>
      <span className="probe-warns">
        {warns.map((w) => (
          <Pill key={w.code} tone={BAD.has(w.code) ? 'warn' : 'info'} title={w.text}>
            ⚠ {LABEL[w.code] ?? w.code}
          </Pill>
        ))}
      </span>
    </footer>
  );
}