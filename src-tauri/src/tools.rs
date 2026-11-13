use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub realesrgan: PathBuf,
    pub models: PathBuf,
}

fn sidecar(name: &str) -> PathBuf {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        let p = dir.join(&file);
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(file) // fall back to PATH
}

impl Tools {
    pub fn locate(resource_dir: Option<PathBuf>) -> Self {
        let models = resource_dir.map(|d| d.join("models")).unwrap_or_else(|| PathBuf::from("models"));
        Self {
            ffmpeg: sidecar("ffmpeg"),
            ffprobe: sidecar("ffprobe"),
            realesrgan: sidecar("realesrgan-ncnn-vulkan"),
            models,
        }
    }
    pub fn has_ai(&self) -> bool {
        self.realesrgan.is_file() && self.models.is_dir()
    }
    pub fn has_ffmpeg(&self) -> bool {
        command(&self.ffmpeg)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

pub fn command(p: &Path) -> Command {
    let c = Command::new(p);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    c
}
