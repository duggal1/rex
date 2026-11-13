export function TitleBar(p: { mac: boolean; activeJobs: number; motion: boolean; onMotion: () => void }) {
  return (
    <header className={`titlebar${p.mac ? ' mac' : ''}`} data-tauri-drag-region>
      <span className="brand" data-tauri-drag-region>
        <img className="brand-logo" src="/logo.svg" alt="Rex" />
        <span>Rex</span>
      </span>
      <span className="spacer" data-tauri-drag-region />
      <button type="button" className="textbtn micro" aria-pressed={p.motion} onClick={p.onMotion}>
        (Motion: {p.motion ? 'on' : 'off'})
      </button>
      {p.activeJobs > 0 && <span className="micro">(Queue · {p.activeJobs})</span>}
    </header>
  );
}
