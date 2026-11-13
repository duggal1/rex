use crate::{
    caps::{self, Caps},
    error::AppError,
    pipeline::{self, Ctx, Proc},
    probe::{self, ProbeInfo},
    queue::{JobRec, JobState, JobView, Queue},
    tools::{command, Tools},
    types::*,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Instant,
};
use tauri::{AppHandle, Manager, State};

pub struct AppState {
    pub queue: Arc<Queue>,
    pub tools: Tools,
    pub caps: Mutex<Option<Caps>>,
    pub bench: Mutex<HashMap<String, f64>>,
}

fn join_err(e: tauri::Error) -> AppError {
    AppError::internal(e.to_string())
}

async fn caps_of(st: &AppState) -> Result<Caps, AppError> {
    if let Some(c) = st.caps.lock().unwrap().clone() {
        return Ok(c);
    }
    let tools = st.tools.clone();
    let c = tauri::async_runtime::spawn_blocking(move || caps::detect(&tools))
        .await
        .map_err(join_err)?;
    *st.caps.lock().unwrap() = Some(c.clone());
    Ok(c)
}

async fn probe_async(tools: &Tools, path: String) -> Result<ProbeInfo, AppError> {
    let ff = tools.ffprobe.clone();
    let r = tauri::async_runtime::spawn_blocking(move || probe::probe(&ff, &path))
        .await
        .map_err(join_err)?;
    r
}

fn human(b: u64) -> String {
    let g = b as f64 / 1e9;
    if g >= 1.0 {
        format!("{g:.1} GB")
    } else {
        format!("{:.0} MB", b as f64 / 1e6)
    }
}

fn free_space(dir: &Path) -> u64 {
    let canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let canon = PathBuf::from(
        canon
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_string(),
    );
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else {
        return u64::MAX;
    };
    disks
        .iter()
        .filter(|d| canon.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
        .unwrap_or(u64::MAX)
}

fn preflight(spec: &JobSpec, plan: &pipeline::Plan) -> Result<(), AppError> {
    let out = PathBuf::from(&spec.encode.out);
    let dir = out
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::new("E_OUTPUT", format!("Can't create the output folder: {e}")))?;
    let probe_file = dir.join(".rex-write-test");
    fs::write(&probe_file, b"x")
        .map_err(|e| AppError::new("E_OUTPUT", format!("Can't write to the output folder: {e}")))?;
    let _ = fs::remove_file(&probe_file);
    let need = (plan.est_bytes as f64 * 1.2) as u64
        + if plan.ai_model.is_some() {
            3_000_000_000
        } else {
            0
        };
    let free = free_space(&dir);
    if free < need {
        return Err(AppError::new(
            "E_DISK_FULL",
            format!(
                "This needs about {} free and only {} is available.",
                human(need),
                human(free)
            ),
        ));
    }
    Ok(())
}

// ───────────────────────── commands ─────────────────────────

#[tauri::command]
pub async fn get_caps(state: State<'_, AppState>) -> Result<Caps, AppError> {
    caps_of(&state).await
}

#[tauri::command]
pub async fn probe_file(state: State<'_, AppState>, path: String) -> Result<ProbeInfo, AppError> {
    probe_async(&state.tools, path).await
}

#[tauri::command]
pub fn default_output(
    path: String,
    profile: Profile,
    long_edge: u32,
    dir: Option<String>,
) -> String {
    let p = PathBuf::from(&path);
    let dir = dir
        .map(PathBuf::from)
        .or_else(|| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".into());
    let ext = if profile == Profile::Edit {
        "mov"
    } else {
        "mp4"
    };
    let mut n = 0;
    loop {
        let name = if n == 0 {
            format!("{stem} upscaled {long_edge}.{ext}")
        } else {
            format!("{stem} upscaled {long_edge} ({}).{ext}", n + 1)
        };
        let c = dir.join(name);
        if !c.exists() {
            return c.to_string_lossy().to_string();
        }
        n += 1;
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    frames: u64,
    bytes: u64,
    out_w: u32,
    out_h: u32,
    out_fps: f64,
    ai_model: Option<String>,
    ai_scale: u32,
    content_scale: f64,
    warnings: Vec<String>,
}

#[tauri::command]
pub async fn estimate(state: State<'_, AppState>, spec: JobSpec) -> Result<Estimate, AppError> {
    let caps = caps_of(&state).await?;
    let probe = probe_async(&state.tools, spec.input.path.clone()).await?;
    let plan = pipeline::plan(&spec, &probe, &caps, &state.tools)?;
    Ok(Estimate {
        frames: plan.total_frames,
        bytes: plan.est_bytes,
        out_w: plan.out_w,
        out_h: plan.out_h,
        out_fps: plan.out_fps,
        ai_model: plan.ai_model,
        ai_scale: plan.ai_scale,
        content_scale: plan.content_scale,
        warnings: plan.warnings,
    })
}

/// Real micro-benchmark: runs ~1 second of the actual pipeline and reports frames/sec.
/// Includes model start-up, so it's pessimistic for short clips. Cached per shape.
#[tauri::command]
pub async fn bench(state: State<'_, AppState>, spec: JobSpec) -> Result<Option<f64>, AppError> {
    if state.queue.is_busy() {
        return Ok(None);
    }
    let caps = caps_of(&state).await?;
    let tools = state.tools.clone();
    let probe = probe_async(&tools, spec.input.path.clone()).await?;

    let mut s = spec.clone();
    s.preview = false;
    s.input.range.end_sec = Some(s.input.range.start_sec + 1.0);
    let ext = if s.encode.profile == Profile::Edit {
        "mov"
    } else {
        "mp4"
    };
    s.encode.out = std::env::temp_dir()
        .join(format!("rex-bench-{}.{ext}", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .to_string();
    let plan = pipeline::plan(&s, &probe, &caps, &tools)?;
    let key = format!(
        "{:?}|{}x{}|{}x{}|{:.2}|{}|{:?}",
        plan.ai_model,
        probe.width,
        probe.height,
        plan.out_w,
        plan.out_h,
        plan.out_fps,
        plan.encoder,
        s.smooth.mode
    );
    if let Some(f) = state.bench.lock().unwrap().get(&key) {
        return Ok(Some(*f));
    }
    let frames = plan.total_frames as f64;
    let out = s.encode.out.clone();
    let ctx = Ctx::new(None, tools, Proc::new(), "bench".into());
    let t = Instant::now();
    let r = tauri::async_runtime::spawn_blocking(move || {
        pipeline::execute(&ctx, &s, &probe, &plan).map(|_| ())
    })
    .await
    .map_err(join_err)?;
    let _ = fs::remove_file(&out);
    if r.is_err() {
        return Ok(None);
    }
    let fps = frames / t.elapsed().as_secs_f64().max(0.05);
    state.bench.lock().unwrap().insert(key, fps);
    Ok(Some(fps))
}

#[tauri::command]
pub async fn enqueue(
    app: AppHandle,
    state: State<'_, AppState>,
    spec: JobSpec,
) -> Result<JobView, AppError> {
    let caps = caps_of(&state).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut spec = spec;
    if spec.preview {
        let dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| AppError::internal(e.to_string()))?
            .join("previews");
        fs::create_dir_all(&dir)?;
        spec.encode.out = dir.join(format!("{id}.mp4")).to_string_lossy().to_string();
        spec.encode.profile = Profile::Compat;
        spec.upscale.long_edge = spec.upscale.long_edge.min(3840);
        spec.input.range.end_sec = Some(spec.input.range.start_sec + 3.0);
    }
    let probe = probe_async(&state.tools, spec.input.path.clone()).await?;
    let plan = pipeline::plan(&spec, &probe, &caps, &state.tools)?;
    preflight(&spec, &plan)?;
    let rec = JobRec {
        id,
        spec,
        probe,
        plan,
        state: JobState::Queued,
        out: None,
        error: None,
        stats: None,
    };
    Ok(state.queue.push(rec))
}

#[tauri::command]
pub fn cancel_job(app: AppHandle, state: State<'_, AppState>, id: String) {
    state.queue.cancel(&app, &id);
}

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Vec<JobView> {
    state.queue.list()
}

/// Extracts one still. With `fit` it renders the source the way a plain player would
/// (bicubic, padded to the result's canvas) so the wipe lines up with the result.
#[tauri::command]
pub async fn extract_frame(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    t: f64,
    fit: Option<(u32, u32)>,
) -> Result<String, AppError> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::internal(e.to_string()))?
        .join("frames");
    fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{}.png", uuid::Uuid::new_v4()));
    let ff = state.tools.ffmpeg.clone();
    let out_s = out.to_string_lossy().to_string();
    let r = tauri::async_runtime::spawn_blocking(move || {
        let mut c = command(&ff);
        c.args(["-hide_banner", "-v", "error", "-y", "-ss", &format!("{:.3}", t.max(0.0)), "-i", &path, "-frames:v", "1"]);
        let vf = match fit {
            Some((w, h)) => format!(
                "scale=iw*sar:ih,setsar=1,scale={w}:{h}:force_original_aspect_ratio=decrease:flags=bicubic,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black"
            ),
            None => "scale=iw*sar:ih,setsar=1".to_string(),
        };
        c.args(["-vf", &vf, &out_s]);
        c.output()
    })
    .await
    .map_err(join_err)??;
    if !r.status.success() {
        return Err(AppError::new(
            "E_DECODE",
            String::from_utf8_lossy(&r.stderr).trim().to_string(),
        ));
    }
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command]
pub fn reveal(path: String) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").args(["-R", &path]).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path))
            .spawn()?;
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let dir = Path::new(&path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        Command::new("xdg-open").arg(dir).spawn()?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(&path).spawn()?;
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("xdg-open").arg(&path).spawn()?;
    }
    Ok(())
}

fn prefs_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    let d = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::internal(e.to_string()))?;
    fs::create_dir_all(&d)?;
    Ok(d.join("prefs.json"))
}

#[tauri::command]
pub fn get_prefs(app: AppHandle) -> serde_json::Value {
    prefs_path(&app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

#[tauri::command]
pub fn set_prefs(app: AppHandle, prefs: serde_json::Value) -> Result<(), AppError> {
    fs::write(prefs_path(&app)?, serde_json::to_string_pretty(&prefs)?)?;
    Ok(())
}
