use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub box_style: usize,
    #[serde(default)]
    pub pad: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            box_style: 1,
            pad: false, // Default is dense (no extra padding)
        }
    }
}

fn get_state_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config/rs-agent/state.json"))
        .unwrap_or_else(|| PathBuf::from("state.json"))
}

pub fn get_box_style() -> usize {
    let path = get_state_path();
    if let Ok(content) = fs::read_to_string(path) {
        if let Ok(state) = serde_json::from_str::<AppState>(&content) {
            if state.box_style >= 1 && state.box_style <= 5 {
                return state.box_style;
            }
        }
    }
    1
}

pub fn set_box_style(style: usize) {
    let path = get_state_path();
    let mut state = if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str::<AppState>(&content).unwrap_or_default()
    } else {
        AppState::default()
    };
    state.box_style = style;
    if let Ok(content) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(path, content);
    }
}

pub fn get_pad() -> bool {
    let path = get_state_path();
    if let Ok(content) = fs::read_to_string(path) {
        if let Ok(state) = serde_json::from_str::<AppState>(&content) {
            return state.pad;
        }
    }
    false // Dense by default
}

pub fn set_pad(pad: bool) {
    let path = get_state_path();
    let mut state = if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str::<AppState>(&content).unwrap_or_default()
    } else {
        AppState::default()
    };
    state.pad = pad;
    if let Ok(content) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(path, content);
    }
}
