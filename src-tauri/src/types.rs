use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Fast,
    Clean,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SmoothMode {
    Off,
    X2,
    Fps,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum Canvas {
    #[serde(rename = "original")]
    Original,
    #[serde(rename = "16:9")]
    W16x9,
    #[serde(rename = "9:16")]
    H9x16,
    #[serde(rename = "1:1")]
    Square,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Fill {
    Black,
    Blur,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Share,
    Edit,
    Compat,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    High,
    Balanced,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    pub v: u8,
    pub input: InputSpec,
    pub upscale: UpscaleSpec,
    pub smooth: SmoothSpec,
    pub frame: FrameSpec,
    pub encode: EncodeSpec,
    pub preview: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputSpec {
    pub path: String,
    pub range: RangeSpec,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeSpec {
    pub start_sec: f64,
    pub end_sec: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpscaleSpec {
    pub mode: Mode,
    pub max_ai_scale: u32,
    pub long_edge: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmoothSpec {
    pub mode: SmoothMode,
    pub fps: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSpec {
    pub canvas: Canvas,
    pub fill: Fill,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeSpec {
    pub profile: Profile,
    pub quality: Quality,
    pub out: String,
}
