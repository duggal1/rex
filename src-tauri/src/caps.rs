use crate::tools::{command, Tools};
use serde::Serialize;
use std::{collections::HashSet, process::Stdio};

#[derive(Debug, Clone, Serialize)]
pub struct Encoders {
    pub share: Option<String>,
    pub edit: Option<String>,
    pub compat: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaxEdge {
    pub share: u32,
    pub edit: u32,
    pub compat: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Caps {
    pub os: String,
    pub ffmpeg: bool,
    pub ai: bool,
    pub encoders: Encoders,
    pub max_edge: MaxEdge,
}

const HEVC: &[&str] = &[
    "hevc_videotoolbox",
    "hevc_nvenc",
    "hevc_qsv",
    "hevc_amf",
    "hevc_mf",
    "libx265",
];
const H264: &[&str] = &[
    "h264_videotoolbox",
    "h264_nvenc",
    "h264_qsv",
    "h264_amf",
    "h264_mf",
    "libx264",
];
const PRORES: &[&str] = &["prores_videotoolbox", "prores_ks"];

fn try_enc(tools: &Tools, enc: &str, w: u32, h: u32, pix: &str) -> bool {
    command(&tools.ffmpeg)
        .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("color=c=black:s={w}x{h}:r=1"))
        .args([
            "-frames:v",
            "1",
            "-c:v",
            enc,
            "-b:v",
            "20M",
            "-pix_fmt",
            pix,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Picks the first encoder that really works (a listed encoder can still fail without the hardware),
/// and finds the largest long edge it can encode.
fn pick(
    tools: &Tools,
    list: &[&str],
    listed: &HashSet<String>,
    pix: &str,
) -> (Option<String>, u32) {
    let mut best: (Option<String>, u32) = (None, 0);
    for enc in list {
        if !listed.contains(*enc) {
            continue;
        }
        for edge in [7680u32, 3840, 1920] {
            let h = (edge * 9 / 16) / 2 * 2;
            if try_enc(tools, enc, edge, h, pix) {
                if edge > best.1 {
                    best = (Some(enc.to_string()), edge);
                }
                break;
            }
        }
    }
    best
}

pub fn detect(tools: &Tools) -> Caps {
    let ffmpeg = tools.has_ffmpeg();
    let listed: HashSet<String> = if ffmpeg {
        command(&tools.ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| {
                        let mut it = l.split_whitespace();
                        let flags = it.next()?;
                        if flags.starts_with('V') {
                            it.next().map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashSet::new()
    };
    let (share, e1) = pick(tools, HEVC, &listed, "yuv420p");
    let (compat, e3) = pick(tools, H264, &listed, "yuv420p");
    let (edit, e2) = pick(tools, PRORES, &listed, "yuv422p10le");
    Caps {
        os: std::env::consts::OS.to_string(),
        ffmpeg,
        ai: tools.has_ai(),
        encoders: Encoders {
            share,
            edit,
            compat,
        },
        max_edge: MaxEdge {
            share: e1,
            edit: e2,
            compat: e3.min(3840),
        },
    }
}
