use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    pub box_style: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self { box_style: 1 }
    }
}

fn state_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config/pyra-agent/state.json"))
        .unwrap_or_else(|| PathBuf::from("state.json"))
}

pub fn get_box_style() -> usize {
    let p = state_path();
    if p.exists() {
        if let Ok(c) = fs::read_to_string(&p) {
            if let Ok(s) = serde_json::from_str::<AppState>(&c) {
                return s.box_style.clamp(1, 3);
            }
        }
    }
    1
}

pub fn set_box_style(style: usize) {
    let p = state_path();
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let s = AppState {
        box_style: style.clamp(1, 3),
    };
    if let Ok(c) = serde_json::to_string_pretty(&s) {
        let _ = fs::write(p, c);
    }
}
