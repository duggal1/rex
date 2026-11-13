import { PixelField } from './PixelField';
import { Btn, Kbd, MOD } from './ui';

export function DropZone(p: { probing: boolean; hot: boolean; onPick: () => void }) {
  return (
    <div className="drop">
      <PixelField mode={p.probing ? 'loading' : 'idle'} hot={p.hot} />
      <div className="drop-card">
        <div className="drop-mark">
          <img className="drop-logo" src="/logo.svg" alt="Rex" />
        </div>
        {p.probing ? (
          <p className="lead">(Probing…)</p>
        ) : (
          <>
            <p className="lead">{p.hot ? 'Release to open it.' : 'Drop a video here.'}</p>
            <p className="note">Low-res in, bigger out. Smoother, not new detail.</p>
            <Btn onClick={p.onPick}>
              (Open file) <Kbd>{MOD}O</Kbd>
            </Btn>
          </>
        )}
      </div>
    </div>
  );
}
