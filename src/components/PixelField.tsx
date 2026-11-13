import { useMemo } from 'react';
import { mulberry } from '../lib/pixel';

interface Props {
  mode: 'idle' | 'loading' | 'progress';
  hot?: boolean;
  progress?: number;
  cell?: number;
}

export function PixelField({ mode, hot = false, progress = 0, cell = 10 }: Props) {
  const pixels = useMemo(() => {
    const random = mulberry(2718);
    return Array.from({ length: 120 }, (_, index) => ({
      x: (index % 12) * cell,
      y: Math.floor(index / 12) * cell,
      opacity: 0.12 + random() * 0.28,
    }));
  }, [cell]);
  const active = Math.round(Math.max(0, Math.min(1, progress)) * pixels.length);

  return (
    <svg
      className={`pixel-field ${mode}${hot ? ' hot' : ''}`}
      viewBox={`0 0 ${cell * 12} ${cell * 10}`}
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      {pixels.map((pixel, index) => (
        <rect
          key={index}
          x={pixel.x}
          y={pixel.y}
          width={cell - 1}
          height={cell - 1}
          opacity={mode === 'progress' && index < active ? 0.72 : pixel.opacity}
        />
      ))}
    </svg>
  );
}
