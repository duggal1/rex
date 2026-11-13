use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub msg: String,
    pub action: &'static str,
}

impl AppError {
    pub fn new(code: &'static str, msg: impl Into<String>) -> Self {
        let action = match code {
            "E_NO_FFMPEG" => "Reinstall the app, or put ffmpeg and ffprobe on your PATH.",
            "E_UNSUPPORTED_INPUT" => "Try another file, or re-export it as H.264 MP4 first.",
            "E_DECODE" => "The file looks damaged. Re-download it or remux it with ffmpeg -c copy.",
            "E_MODEL_UNAVAILABLE" => "Reinstall the app so the AI models are present, and make sure your GPU supports Vulkan or Metal.",
            "E_ENCODER_LIMIT" => "Pick a smaller size or the Compatible export.",
            "E_DISK_FULL" => "Free some disk space or pick another output folder.",
            "E_OOM_GUARD" => "Close other apps or pick a smaller size.",
            "E_OUTPUT" => "Pick a folder you can write to.",
            "E_CANCELLED" => "Nothing to fix. You cancelled it.",
            _ => "Retry. If it keeps happening, copy the details and report it.",
        };
        Self { code, msg: msg.into(), action }
    }
    pub fn cancelled() -> Self {
        Self::new("E_CANCELLED", "Job cancelled.")
    }
    pub fn internal(m: impl Into<String>) -> Self {
        Self::new("E_INTERNAL", m)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(e.to_string())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::internal(e.to_string())
    }
}