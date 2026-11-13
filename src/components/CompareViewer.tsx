import { useEffect, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';

type Flip = null | 'src' | 'res';

/** Wipe between source and result. `\` flips A/B, Z cycles zoom, drag to pan when zoomed. */
export function CompareViewer({ src, res }: { src?: string; res?: string }) {
  const [wipe, setWipe] = useState(0.5);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [flip, setFlip] = useState<Flip>(null);
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<null | { k: 'wipe' | 'pan'; x: number; y: number; px: number; py: number }>(null);
  const both = !!src && !!res;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.tagName === 'INPUT') return;
      if (e.key === '\\') setFlip((f) => (f === null ? 'src' : f === 'src' ? 'res' : null));
      if (e.key.toLowerCase() === 'z') {
        setZoom((z) => (z >= 8 ? 1 : z * 2));
        setPan({ x: 0, y: 0 });
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const down = (e: ReactPointerEvent<HTMLDivElement>) => {
    const onHandle = e.target instanceof Element && e.target.closest('[data-handle]');
    if (onHandle && both) drag.current = { k: 'wipe', x: 0, y: 0, px: 0, py: 0 };
    else if (zoom > 1) drag.current = { k: 'pan', x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
    else return;
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const move = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    if (d.k === 'wipe') {
      const element = box.current;
      if (!element) return;
      const r = element.getBoundingClientRect();
      setWipe(Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)));
    } else setPan({ x: d.px + e.clientX - d.x, y: d.py + e.clientY - d.y });
  };
  const up = () => {
    drag.current = null;
  };

  const w = flip === 'src' ? 1 : flip === 'res' ? 0 : both ? wipe : 1;
  const style = {
    transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
    imageRendering: zoom > 1 ? ('pixelated' as const) : ('auto' as const),
  };

  return (
    <div ref={box} className="cmp" onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up}>
      {res && (
        <div className="layer">
          <img src={res} alt="" draggable={false} style={style} />
        </div>
      )}
      {src && (
        <div className="layer" style={both ? { clipPath: `inset(0 ${(1 - w) * 100}% 0 0)` } : undefined}>
          <img src={src} alt="" draggable={false} style={style} />
        </div>
      )}
      {both && !flip && (
        <div className="handle" data-handle style={{ left: `${wipe * 100}%` }}>
          <i />
        </div>
      )}
      <span className="tag tl micro">(Source)</span>
      {res && <span className="tag tr micro">(Result)</span>}
      <span className="tag br micro tnum">{zoom}×{flip ? ` · ${flip === 'src' ? 'A' : 'B'}` : ''}</span>
    </div>
  );
}
