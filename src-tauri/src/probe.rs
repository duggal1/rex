use crate::{error::AppError, tools::command};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub code: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    pub path: String,
    pub container: String,
    pub duration_sec: f64,
    pub width: u32,  // after rotation, before SAR
    pub height: u32,
    pub display_width: u32, // square-pixel
    pub display_height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub fps: f64,
    pub vfr: bool,
    pub codec: String,
    pub pix_fmt: String,
    pub interlaced: bool,
    pub sar_num: u32,
    pub sar_den: u32,
    pub rotation: u32,
    pub color_space: String,
    pub matrix: String,
    pub hdr: bool,
    pub has_audio: bool,
    pub audio_codecs: Vec<String>,
    pub size_bytes: u64,
    pub bitrate: u64,
    pub warnings: Vec<Warning>,
}

fn st(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}
fn ratio(t: &str) -> Option<(u32, u32)> {
    let (a, b) = t.split_once(|c: char| c == '/' || c == ':')?;
    let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
    if a > 0 && b > 0 { Some((a, b)) } else { None }
}
fn w(code: &str, text: &str) -> Warning {
    Warning { code: code.into(), text: text.into() }
}

pub fn probe(ffprobe: &Path, path: &str) -> Result<ProbeInfo, AppError> {
    let out = command(ffprobe)
        .args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format", path])
        .output()
        .map_err(|e| AppError::new("E_NO_FFMPEG", format!("Could not run ffprobe: {e}")))?;
    if !out.status.success() {
        return Err(AppError::new(
            "E_UNSUPPORTED_INPUT",
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    let j: Value = serde_json::from_slice(&out.stdout)?;
    let streams = j["streams"].as_array().cloned().unwrap_or_default();
    let vs = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"].as_i64().unwrap_or(0) == 0)
        .ok_or_else(|| AppError::new("E_UNSUPPORTED_INPUT", "No video stream found in this file."))?;

    let (cw, ch) = (vs["width"].as_u64().unwrap_or(0) as u32, vs["height"].as_u64().unwrap_or(0) as u32);
    if cw == 0 || ch == 0 {
        return Err(AppError::new("E_UNSUPPORTED_INPUT", "The video stream has no dimensions."));
    }

    let mut rot: i64 = 0;
    if let Some(arr) = vs["side_data_list"].as_array() {
        for sd in arr {
            if let Some(r) = sd["rotation"].as_f64() {
                rot = r as i64;
            }
        }
    }
    if rot == 0 {
        if let Some(r) = vs["tags"]["rotate"].as_str().and_then(|r| r.parse::<i64>().ok()) {
            rot = r;
        }
    }
    let rotation = (((rot % 360) + 360) % 360) as u32;
    let (width, height) = if rotation == 90 || rotation == 270 { (ch, cw) } else { (cw, ch) };

    let (sar_num, sar_den) = ratio(&st(vs, "sample_aspect_ratio")).unwrap_or((1, 1));
    let sar = sar_num as f64 / sar_den as f64;
    let display_width = ((width as f64) * sar).round().max(1.0) as u32;
    let display_height = height;

    let avg = ratio(&st(vs, "avg_frame_rate"));
    let rr = ratio(&st(vs, "r_frame_rate"));
    let (fps_num, fps_den) = avg.or(rr).unwrap_or((30, 1));
    let fps = fps_num as f64 / fps_den as f64;
    let vfr = match (avg, rr) {
        (Some(a), Some(b)) => {
            let (a, b) = (a.0 as f64 / a.1 as f64, b.0 as f64 / b.1 as f64);
            (a - b).abs() / a.max(0.001) > 0.01
        }
        _ => false,
    };

    let field = st(vs, "field_order");
    let interlaced = matches!(field.as_str(), "tt" | "bb" | "tb" | "bt");
    let cs = st(vs, "color_space");
    let color_space = if cs == "unknown" { String::new() } else { cs };
    let matrix = match color_space.as_str() {
        "bt709" => "bt709",
        "smpte170m" | "bt470bg" => "bt601",
        "bt2020nc" | "bt2020c" => "bt2020",
        _ => if ch <= 576 { "bt601" } else { "bt709" },
    }
    .to_string();
    let trc = st(vs, "color_transfer");
    let hdr = trc == "smpte2084" || trc == "arib-std-b67";

    let audio_codecs: Vec<String> = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .map(|s| st(s, "codec_name"))
        .collect();

    let fmt = &j["format"];
    let duration_sec = st(vs, "duration")
        .parse::<f64>()
        .or_else(|_| st(fmt, "duration").parse::<f64>())
        .unwrap_or(0.0);

    let mut warnings = vec![];
    if interlaced {
        warnings.push(w("interlaced_detected", "Interlaced source. Deinterlacing automatically."));
    }
    if (sar - 1.0).abs() > 0.01 {
        warnings.push(w("non_square_pixels", "Non-square pixels. Correcting the aspect ratio."));
    }
    if rotation != 0 {
        warnings.push(w("rotated", "Rotation metadata found. The output is rotated upright."));
    }
    if vfr {
        warnings.push(w("vfr", "Variable frame rate. Converting to constant."));
    }
    if color_space.is_empty() && ch <= 576 {
        warnings.push(w("sd_matrix", "Untagged SD video. Assuming BT.601 and converting to BT.709."));
    }
    if hdr {
        warnings.push(w("hdr_unsupported", "HDR source. Output is SDR, so colors will look flat."));
    }
    if width.max(height) < 240 {
        warnings.push(w("tiny_source", "Very small source. Expect soft results."));
    }

    Ok(ProbeInfo {
        path: path.to_string(),
        container: st(fmt, "format_name"),
        duration_sec,
        width,
        height,
        display_width,
        display_height,
        fps_num,
        fps_den,
        fps,
        vfr,
        codec: st(vs, "codec_name"),
        pix_fmt: st(vs, "pix_fmt"),
        interlaced,
        sar_num,
        sar_den,
        rotation,
        color_space,
        matrix,
        hdr,
        has_audio: !audio_codecs.is_empty(),
        audio_codecs,
        size_bytes: st(fmt, "size").parse().unwrap_or(0),
        bitrate: st(fmt, "bit_rate").parse().unwrap_or(0),
        warnings,
    })
}