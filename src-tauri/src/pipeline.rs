use crate::{
    caps::Caps,
    error::AppError,
    probe::ProbeInfo,
    tools::{command, Tools},
    types::*,
};
use serde::Serialize;
use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

const CHUNK: u64 = 96;

// ───────────────────────── events ─────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    #[serde(rename = "elapsedSec")]
    pub elapsed_sec: f64,
    #[serde(rename = "outBytes")]
    pub out_bytes: u64,
    pub frames: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Ev {
    Started,
    Progress {
        stage: String,
        frame: u64,
        total: u64,
        fps: f64,
        #[serde(rename = "etaSec")]
        eta_sec: Option<f64>,
    },
    Warn {
        code: String,
        text: String,
    },
    Done {
        out: String,
        stats: Stats,
    },
    Error {
        error: AppError,
    },
    Cancelled,
}

#[derive(Serialize, Clone)]
struct Payload<'a> {
    id: &'a str,
    ev: &'a Ev,
}

pub fn emit(app: &AppHandle, id: &str, ev: Ev) {
    let _ = app.emit("job-event", &Payload { id, ev: &ev });
}

pub fn warn_text(code: &str) -> &'static str {
    match code {
        "ai_unavailable" => "AI engine or models not found. Using Lanczos resize only.",
        _ => "Heads up.",
    }
}

// ───────────────────────── plan ─────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub src_fps_expr: String,
    pub out_fps: f64,
    pub out_fps_expr: String,
    pub interp: bool,
    pub out_w: u32,
    pub out_h: u32,
    pub start: f64,
    pub dur: f64,
    pub total_frames: u64,
    pub ai_model: Option<String>,
    pub ai_scale: u32,
    pub encoder: String,
    pub bitrate: u64,
    pub est_bytes: u64,
    pub content_scale: f64,
    pub warnings: Vec<String>,
}

fn even(v: f64) -> u32 {
    (((v.round() as i64) / 2) * 2).max(2) as u32
}

pub fn plan(spec: &JobSpec, p: &ProbeInfo, caps: &Caps, tools: &Tools) -> Result<Plan, AppError> {
    if spec.v != 1 {
        return Err(AppError::new(
            "E_UNSUPPORTED_INPUT",
            "This job request version is not supported.",
        ));
    }
    if !caps.ffmpeg {
        return Err(AppError::new("E_NO_FFMPEG", "ffmpeg was not found."));
    }
    let start = spec.input.range.start_sec.max(0.0);
    let end = spec
        .input
        .range
        .end_sec
        .unwrap_or(p.duration_sec)
        .min(p.duration_sec);
    let dur = end - start;
    if dur <= 0.04 {
        return Err(AppError::new(
            "E_UNSUPPORTED_INPUT",
            "This file has no readable duration.",
        ));
    }

    let src_expr = format!("{}/{}", p.fps_num, p.fps_den);
    let (out_fps, out_expr, interp) = match spec.smooth.mode {
        SmoothMode::Off => (p.fps, src_expr.clone(), false),
        SmoothMode::X2 => (
            p.fps * 2.0,
            format!("{}/{}", p.fps_num * 2, p.fps_den),
            true,
        ),
        SmoothMode::Fps => {
            let f = spec.smooth.fps.clamp(24, 120);
            (f as f64, f.to_string(), (f as f64) > p.fps * 1.01)
        }
    };
    let total_frames = ((dur * out_fps).ceil() as u64).max(1);

    let l = spec.upscale.long_edge.clamp(480, 7680) as f64;
    let (dw, dh) = (p.display_width as f64, p.display_height as f64);
    let (w, h) = match spec.frame.canvas {
        Canvas::Original => {
            if dw >= dh {
                (even(l), even(l * dh / dw))
            } else {
                (even(l * dw / dh), even(l))
            }
        }
        Canvas::W16x9 => (even(l), even(l * 9.0 / 16.0)),
        Canvas::H9x16 => (even(l * 9.0 / 16.0), even(l)),
        Canvas::Square => (even(l), even(l)),
    };
    let content_scale = (w as f64 / dw).min(h as f64 / dh);

    let (enc, max_edge) = match spec.encode.profile {
        Profile::Share => (&caps.encoders.share, caps.max_edge.share),
        Profile::Edit => (&caps.encoders.edit, caps.max_edge.edit),
        Profile::Compat => (&caps.encoders.compat, caps.max_edge.compat),
    };
    let encoder = enc.clone().ok_or_else(|| {
        AppError::new(
            "E_ENCODER_LIMIT",
            "No working encoder was found for this export profile on this machine.",
        )
    })?;
    if w.max(h) > max_edge {
        return Err(AppError::new(
            "E_ENCODER_LIMIT",
            format!(
                "The {encoder} encoder on this machine tops out at {max_edge} px on the long edge."
            ),
        ));
    }

    let mut warnings = vec![];
    let (mut ai_model, mut ai_scale) = (None, 0u32);
    if content_scale > 1.05 && spec.upscale.max_ai_scale >= 2 {
        if tools.has_ai() {
            let (m, s) = match spec.upscale.mode {
                Mode::Fast => ("realesr-animevideov3", 2),
                Mode::Clean => {
                    if content_scale > 2.0 && spec.upscale.max_ai_scale >= 4 {
                        ("realesrgan-x4plus", 4)
                    } else {
                        ("realesr-animevideov3", 2)
                    }
                }
            };
            ai_model = Some(m.to_string());
            ai_scale = s;
        } else {
            warnings.push("ai_unavailable".to_string());
        }
    }

    let q = if spec.encode.quality == Quality::Balanced {
        0.65
    } else {
        1.0
    };
    let bpp = match spec.encode.profile {
        Profile::Share => 0.06 * q,
        Profile::Compat => 0.10 * q,
        Profile::Edit => 2.4,
    };
    let bitrate = ((w as f64) * (h as f64) * out_fps * bpp) as u64;
    let est_bytes = (bitrate as f64 * dur / 8.0) as u64 + (dur * 24_000.0) as u64;

    Ok(Plan {
        src_fps_expr: src_expr,
        out_fps,
        out_fps_expr: out_expr,
        interp,
        out_w: w,
        out_h: h,
        start,
        dur,
        total_frames,
        ai_model,
        ai_scale,
        encoder,
        bitrate,
        est_bytes,
        content_scale,
        warnings,
    })
}

// ───────────────────────── filters & args ─────────────────────────

fn pix_fmt(p: Profile) -> &'static str {
    if p == Profile::Edit {
        "yuv422p10le"
    } else {
        "yuv420p"
    }
}

pub fn cleanup_chain(p: &ProbeInfo, plan: &Plan, rgb: bool) -> String {
    let mut f: Vec<String> = vec![];
    if p.interlaced {
        f.push("bwdif=mode=send_frame:parity=auto:deint=all".into());
    }
    f.push(format!(
        "scale=iw*sar:ih:flags=bicubic:in_color_matrix={}:out_color_matrix=bt709",
        p.matrix
    ));
    f.push("setsar=1".into());
    if plan.interp {
        f.push(format!("fps={}", plan.src_fps_expr));
        f.push(format!(
            "minterpolate=fps={}:mi_mode=mci:mc_mode=aobmc:me_mode=bidir:vsbmc=1:scd=fdiff:scd_threshold=8",
            plan.out_fps_expr
        ));
    } else {
        f.push(format!("fps={}", plan.out_fps_expr));
    }
    if rgb {
        f.push("format=rgb24".into());
    }
    f.join(",")
}

pub fn compose_chain(plan: &Plan, fill: Fill, pix: &str, inl: &str, outl: &str) -> String {
    let (w, h) = (plan.out_w, plan.out_h);
    match fill {
        Fill::Black => format!(
            "[{inl}]scale={w}:{h}:force_original_aspect_ratio=decrease:flags=lanczos:out_color_matrix=bt709:out_range=tv,format={pix},pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black[{outl}]"
        ),
        Fill::Blur => {
            let (bw, bh) = (even(w as f64 / 8.0), even(h as f64 / 8.0));
            format!(
                "[{inl}]split=2[bga][fga];\
                 [bga]scale={bw}:{bh}:force_original_aspect_ratio=increase,crop={bw}:{bh},boxblur=4:2,scale={w}:{h}:flags=bilinear,format={pix}[bg];\
                 [fga]scale={w}:{h}:force_original_aspect_ratio=decrease:flags=lanczos:out_color_matrix=bt709:out_range=tv,format={pix}[fg];\
                 [bg][fg]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2[{outl}]"
            )
        }
    }
}

fn base() -> Vec<String> {
    ["-hide_banner", "-nostdin", "-y", "-v", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}
fn a(v: &mut Vec<String>, xs: &[&str]) {
    v.extend(xs.iter().map(|s| s.to_string()));
}
fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

fn venc_args(plan: &Plan, q: Quality) -> Vec<String> {
    let enc = plan.encoder.as_str();
    let br = plan.bitrate.to_string();
    let bal = q == Quality::Balanced;
    let mut v: Vec<String> = vec!["-c:v".into(), enc.into()];
    let mut add = |xs: &[&str]| v.extend(xs.iter().map(|x| x.to_string()));
    match enc {
        e if e.starts_with("prores") => {
            add(&["-profile:v", "standard"]);
            if e == "prores_ks" {
                add(&["-vendor", "apl0"]);
            }
        }
        "libx265" => add(&[
            "-crf",
            if bal { "24" } else { "20" },
            "-preset",
            "medium",
            "-tag:v",
            "hvc1",
        ]),
        "libx264" => add(&[
            "-crf",
            if bal { "21" } else { "18" },
            "-preset",
            "medium",
            "-profile:v",
            "high",
        ]),
        e if e.starts_with("hevc_") => {
            add(&["-b:v", &br, "-tag:v", "hvc1"]);
            if e == "hevc_nvenc" {
                add(&["-preset", "p5"]);
            }
            if e == "hevc_videotoolbox" {
                add(&["-allow_sw", "1"]);
            }
        }
        e if e.starts_with("h264_") => {
            add(&["-b:v", &br]);
            if e == "h264_nvenc" {
                add(&["-preset", "p5"]);
            }
            if e != "h264_mf" {
                add(&["-profile:v", "high"]);
            }
        }
        _ => add(&["-b:v", &br]),
    }
    v
}

fn color_args() -> Vec<String> {
    [
        "-colorspace",
        "bt709",
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        "-color_range",
        "tv",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn audio_args(p: &ProbeInfo, profile: Profile) -> Vec<String> {
    let mut v = vec![];
    if !p.has_audio {
        a(&mut v, &["-an"]);
        return v;
    }
    let copy_ok = p.audio_codecs.iter().all(|c| match c.as_str() {
        "aac" | "mp3" | "ac3" | "eac3" | "alac" => true,
        c if c.starts_with("pcm_") => profile == Profile::Edit,
        _ => false,
    });
    if copy_ok {
        a(&mut v, &["-c:a", "copy"]);
    } else if profile == Profile::Edit {
        a(&mut v, &["-c:a", "pcm_s16le"]);
    } else {
        a(&mut v, &["-c:a", "aac", "-b:a", "192k"]);
    }
    v
}

// ───────────────────────── process supervision ─────────────────────────

#[derive(Clone)]
pub struct Proc {
    slot: Arc<Mutex<Option<Child>>>,
    pub cancel: Arc<AtomicBool>,
}

fn classify(tail: &str, fallback: &'static str) -> AppError {
    let t = tail.to_lowercase();
    let code = if t.contains("no space left") || t.contains("disk full") {
        "E_DISK_FULL"
    } else if t.contains("cannot allocate memory") || t.contains("out of memory") {
        "E_OOM_GUARD"
    } else {
        fallback
    };
    let lines: Vec<&str> = tail.lines().collect();
    let msg = lines[lines.len().saturating_sub(4)..].join("\n");
    AppError::new(
        code,
        if msg.is_empty() {
            "The process exited with an error.".to_string()
        } else {
            msg
        },
    )
}

impl Proc {
    pub fn new() -> Self {
        Self {
            slot: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(c) = self.slot.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }

    fn start(
        &self,
        mut cmd: Command,
        pipe: bool,
    ) -> Result<(Option<ChildStdout>, JoinHandle<String>), AppError> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(AppError::cancelled());
        }
        cmd.stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(if pipe { Stdio::piped() } else { Stdio::null() });
        let mut child = cmd.spawn().map_err(|e| {
            AppError::new(
                "E_NO_FFMPEG",
                format!("Could not start a helper process: {e}"),
            )
        })?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        *self.slot.lock().unwrap() = Some(child);
        if self.cancel.load(Ordering::SeqCst) {
            self.cancel();
        }
        let h = thread::spawn(move || {
            let mut buf: VecDeque<String> = VecDeque::new();
            if let Some(e) = stderr {
                for l in BufReader::new(e).lines().map_while(Result::ok) {
                    buf.push_back(l);
                    if buf.len() > 40 {
                        buf.pop_front();
                    }
                }
            }
            buf.into_iter().collect::<Vec<_>>().join("\n")
        });
        Ok((stdout, h))
    }

    fn wait(
        &self,
        h: JoinHandle<String>,
        tick: &mut dyn FnMut(),
        fallback: &'static str,
    ) -> Result<(), AppError> {
        let status = loop {
            let res = {
                let mut g = self.slot.lock().unwrap();
                match g.as_mut().map(|c| c.try_wait()) {
                    Some(Ok(Some(st))) => {
                        *g = None;
                        Some(Ok(st))
                    }
                    Some(Ok(None)) => None,
                    Some(Err(e)) => {
                        *g = None;
                        Some(Err(e))
                    }
                    None => Some(Err(std::io::Error::other("process lost"))),
                }
            };
            if let Some(r) = res {
                break r;
            }
            tick();
            thread::sleep(Duration::from_millis(60));
        };
        let tail = h.join().unwrap_or_default();
        if self.cancel.load(Ordering::SeqCst) {
            return Err(AppError::cancelled());
        }
        match status {
            Ok(st) if st.success() => Ok(()),
            Ok(_) => Err(classify(&tail, fallback)),
            Err(e) => Err(AppError::internal(e.to_string())),
        }
    }

    pub fn run_lines(
        &self,
        cmd: Command,
        mut on: impl FnMut(&str),
        fallback: &'static str,
    ) -> Result<(), AppError> {
        let (stdout, h) = self.start(cmd, true)?;
        if let Some(o) = stdout {
            for line in BufReader::new(o).lines().map_while(Result::ok) {
                on(&line);
            }
        }
        self.wait(h, &mut || {}, fallback)
    }

    pub fn run_poll(
        &self,
        cmd: Command,
        mut tick: impl FnMut(),
        fallback: &'static str,
    ) -> Result<(), AppError> {
        let (_, h) = self.start(cmd, false)?;
        self.wait(h, &mut tick, fallback)
    }
}

// ───────────────────────── execution ─────────────────────────

pub struct Ctx {
    pub app: Option<AppHandle>,
    pub tools: Tools,
    pub proc: Proc,
    pub id: String,
    started: Instant,
}

impl Ctx {
    pub fn new(app: Option<AppHandle>, tools: Tools, proc: Proc, id: String) -> Self {
        Self {
            app,
            tools,
            proc,
            id,
            started: Instant::now(),
        }
    }
    pub fn ev(&self, ev: Ev) {
        if let Some(a) = &self.app {
            emit(a, &self.id, ev);
        }
    }
    fn progress(&self, stage: &str, frame: u64, total: u64, fps: f64) {
        let fps = if fps > 0.0 {
            fps
        } else {
            frame as f64 / self.started.elapsed().as_secs_f64().max(0.001)
        };
        let eta = if fps > 0.05 && total > frame {
            Some((total - frame) as f64 / fps)
        } else {
            None
        };
        self.ev(Ev::Progress {
            stage: stage.into(),
            frame,
            total,
            fps,
            eta_sec: eta,
        });
    }
    fn ff(&self, args: &[String]) -> Command {
        let mut c = command(&self.tools.ffmpeg);
        c.args(args);
        c
    }
}

fn partial_path(out: &Path) -> PathBuf {
    let stem = out
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out".into());
    let ext = out
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "mp4".into());
    out.with_file_name(format!("{stem}.partial.{ext}"))
}

fn count_files(d: &Path) -> usize {
    fs::read_dir(d).map(|r| r.count()).unwrap_or(0)
}

fn clear_dir(d: &Path) -> Result<(), AppError> {
    for e in fs::read_dir(d)? {
        let _ = fs::remove_file(e?.path());
    }
    Ok(())
}

pub fn execute(ctx: &Ctx, spec: &JobSpec, p: &ProbeInfo, plan: &Plan) -> Result<Stats, AppError> {
    let t0 = Instant::now();
    for wcode in &plan.warnings {
        ctx.ev(Ev::Warn {
            code: wcode.clone(),
            text: warn_text(wcode).to_string(),
        });
    }
    let out = PathBuf::from(&spec.encode.out);
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| AppError::new("E_OUTPUT", e.to_string()))?;
    }
    let partial = partial_path(&out);
    let _ = fs::remove_file(&partial);

    let res = if plan.ai_model.is_some() {
        run_ai(ctx, spec, p, plan, &partial)
    } else {
        run_direct(ctx, spec, p, plan, &partial)
    };
    match res {
        Ok(()) => {
            fs::rename(&partial, &out).map_err(|e| AppError::new("E_OUTPUT", e.to_string()))?;
            Ok(Stats {
                elapsed_sec: t0.elapsed().as_secs_f64(),
                out_bytes: fs::metadata(&out).map(|m| m.len()).unwrap_or(0),
                frames: plan.total_frames,
            })
        }
        Err(e) => {
            let _ = fs::remove_file(&partial);
            Err(e)
        }
    }
}

/// No AI: one ffmpeg process does cleanup, Lanczos, canvas, encode and audio.
fn run_direct(
    ctx: &Ctx,
    spec: &JobSpec,
    p: &ProbeInfo,
    plan: &Plan,
    out: &Path,
) -> Result<(), AppError> {
    let mut args = base();
    a(
        &mut args,
        &["-progress", "pipe:1", "-nostats", "-stats_period", "0.5"],
    );
    if plan.start > 0.0 {
        a(&mut args, &["-ss", &format!("{:.6}", plan.start)]);
    }
    a(
        &mut args,
        &["-t", &format!("{:.6}", plan.dur), "-i", &spec.input.path],
    );
    let graph = format!(
        "[0:v:0]{}[cv];{}",
        cleanup_chain(p, plan, false),
        compose_chain(
            plan,
            spec.frame.fill,
            pix_fmt(spec.encode.profile),
            "cv",
            "vout"
        )
    );
    a(&mut args, &["-filter_complex", &graph, "-map", "[vout]"]);
    if p.has_audio {
        a(&mut args, &["-map", "0:a?"]);
    }
    args.extend(venc_args(plan, spec.encode.quality));
    args.extend(color_args());
    args.extend(audio_args(p, spec.encode.profile));
    a(&mut args, &["-movflags", "+faststart", &s(out)]);

    let (mut frame, mut fps) = (0u64, 0.0f64);
    ctx.proc.run_lines(
        ctx.ff(&args),
        |line| {
            if let Some(v) = line.strip_prefix("frame=") {
                frame = v.trim().parse().unwrap_or(frame);
            } else if let Some(v) = line.strip_prefix("fps=") {
                fps = v.trim().parse().unwrap_or(fps);
            } else if line.starts_with("progress=") {
                ctx.progress(
                    "encode",
                    frame.min(plan.total_frames),
                    plan.total_frames,
                    fps,
                );
            }
        },
        "E_ENCODER_LIMIT",
    )
}

/// AI path: chunked decode → realesrgan (folder mode) → compose+encode segment → concat+mux audio.
fn run_ai(
    ctx: &Ctx,
    spec: &JobSpec,
    p: &ProbeInfo,
    plan: &Plan,
    out: &Path,
) -> Result<(), AppError> {
    let tmp = std::env::temp_dir().join(format!("rex-{}", ctx.id));
    let (din, dout, dseg) = (tmp.join("in"), tmp.join("out"), tmp.join("seg"));
    for d in [&din, &dout, &dseg] {
        fs::create_dir_all(d).map_err(|e| AppError::new("E_OUTPUT", e.to_string()))?;
    }
    let res = ai_chunks(ctx, spec, p, plan, &din, &dout, &dseg)
        .and_then(|n| mux(ctx, spec, p, plan, &dseg, n, out));
    let _ = fs::remove_dir_all(&tmp);
    res
}

fn ai_chunks(
    ctx: &Ctx,
    spec: &JobSpec,
    p: &ProbeInfo,
    plan: &Plan,
    din: &Path,
    dout: &Path,
    dseg: &Path,
) -> Result<usize, AppError> {
    let total = plan.total_frames;
    let n_chunks = total.div_ceil(CHUNK);
    let model = plan.ai_model.clone().unwrap_or_default();
    let models = s(&ctx.tools.models);
    let mut segs = 0usize;

    for k in 0..n_chunks {
        let before = k * CHUNK;
        let want = (total - before).min(CHUNK);
        clear_dir(din)?;
        clear_dir(dout)?;

        // A. decode + cleanup to small PNGs
        let t0 = (plan.start + before as f64 / plan.out_fps - 0.25 / plan.out_fps).max(0.0);
        let mut args = base();
        a(
            &mut args,
            &[
                "-ss",
                &format!("{:.6}", t0),
                "-i",
                &spec.input.path,
                "-an",
                "-vf",
                &cleanup_chain(p, plan, true),
                "-frames:v",
                &want.to_string(),
                "-compression_level",
                "1",
                "-start_number",
                "1",
                &s(&din.join("%06d.png")),
            ],
        );
        ctx.proc.run_lines(ctx.ff(&args), |_| {}, "E_DECODE")?;
        let got = count_files(din);
        if got == 0 {
            if k == 0 {
                return Err(AppError::new(
                    "E_DECODE",
                    "No frames could be decoded from the input.",
                ));
            }
            break;
        }
        ctx.progress("decode", before + got as u64, total, 0.0);

        // B. AI upscale (folder mode). Progress = files appearing in the out dir.
        let mut ai = vec![];
        a(
            &mut ai,
            &[
                "-i",
                &s(din),
                "-o",
                &s(dout),
                "-n",
                &model,
                "-s",
                &plan.ai_scale.to_string(),
                "-m",
                &models,
                "-f",
                "png",
            ],
        );
        let mut c = command(&ctx.tools.realesrgan);
        c.args(&ai);
        let mut last = 0usize;
        ctx.proc.run_poll(
            c,
            || {
                let n = count_files(dout);
                if n != last {
                    last = n;
                    ctx.progress("sr", before + n.min(got) as u64, total, 0.0);
                }
            },
            "E_MODEL_UNAVAILABLE",
        )?;
        let produced = count_files(dout);
        if produced < got {
            return Err(AppError::new(
                "E_MODEL_UNAVAILABLE",
                format!("The AI engine produced {produced} of {got} frames. The GPU may lack Vulkan support or ran out of memory."),
            ));
        }

        // C. compose + encode this chunk to an intermediate segment
        let seg = dseg.join(format!("{:05}.mkv", k));
        let mut e = base();
        a(
            &mut e,
            &[
                "-framerate",
                &plan.out_fps_expr,
                "-start_number",
                "1",
                "-i",
                &s(&dout.join("%06d.png")),
                "-filter_complex",
                &compose_chain(
                    plan,
                    spec.frame.fill,
                    pix_fmt(spec.encode.profile),
                    "0:v:0",
                    "vout",
                ),
                "-map",
                "[vout]",
                "-an",
                "-r",
                &plan.out_fps_expr,
            ],
        );
        e.extend(venc_args(plan, spec.encode.quality));
        e.extend(color_args());
        a(&mut e, &[&s(&seg)]);
        ctx.proc.run_lines(ctx.ff(&e), |_| {}, "E_ENCODER_LIMIT")?;
        segs += 1;
        ctx.progress("encode", before + got as u64, total, 0.0);
    }
    Ok(segs)
}

fn mux(
    ctx: &Ctx,
    spec: &JobSpec,
    p: &ProbeInfo,
    plan: &Plan,
    dseg: &Path,
    segs: usize,
    out: &Path,
) -> Result<(), AppError> {
    let list = dseg.join("list.txt");
    let mut txt = String::new();
    for i in 0..segs {
        let path = dseg.join(format!("{:05}.mkv", i));
        txt.push_str(&format!(
            "file '{}'\n",
            s(&path).replace('\\', "/").replace('\'', "'\\''")
        ));
    }
    fs::write(&list, txt)?;

    let mut m = base();
    a(&mut m, &["-f", "concat", "-safe", "0", "-i", &s(&list)]);
    if p.has_audio {
        if plan.start > 0.0 {
            a(&mut m, &["-ss", &format!("{:.6}", plan.start)]);
        }
        a(
            &mut m,
            &[
                "-t",
                &format!("{:.6}", plan.dur),
                "-i",
                &spec.input.path,
                "-map",
                "0:v:0",
                "-map",
                "1:a?",
            ],
        );
    } else {
        a(&mut m, &["-map", "0:v:0"]);
    }
    a(&mut m, &["-c:v", "copy"]);
    if plan.encoder.starts_with("hevc_") || plan.encoder == "libx265" {
        a(&mut m, &["-tag:v", "hvc1"]);
    }
    m.extend(audio_args(p, spec.encode.profile));
    a(&mut m, &["-movflags", "+faststart", &s(out)]);
    ctx.progress("mux", plan.total_frames, plan.total_frames, 0.0);
    ctx.proc.run_lines(ctx.ff(&m), |_| {}, "E_ENCODER_LIMIT")
}
