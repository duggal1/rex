import { useMemo } from 'react';
import { layout } from '../lib/pixel';

export function BitmapText(p: { text: string; h?: number; bold?: boolean; fluid?: boolean; fill?: string; label?: string }) {
  const L = useMemo(() => layout(p.text, p.bold), [p.text, p.bold]);
  const h = p.h ?? 3;
  const size = p.fluid ? { width: '100%' } : { width: L.cols * h * 1.25, height: L.rows * h };
  return (
    <svg
      viewBox={`0 0 ${L.cols * 5} ${L.rows * 4}`}
      {...size}
      shapeRendering="crispEdges"
      fill={p.fill ?? 'currentColor'}
      role={p.label ? 'img' : undefined}
      aria-label={p.label}
      aria-hidden={p.label ? undefined : true}
      style={{ display: 'block' }}
    >
      {L.runs.map((r, i) => (
        <rect key={i} x={r.x * 5} y={r.y * 4} width={r.w * 5} height={4.04} />
      ))}
    </svg>
  );
}