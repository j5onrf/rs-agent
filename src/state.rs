use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub box_style: usize,
    #[serde(default)]
    pub pad: bool,
}

// =============================================================================
// THE SINGLE SOURCE FOR DEFAULTS:
// =============================================================================
impl Default for AppState {
    fn default() -> Self {
        Self {
            box_style: 1,
            pad: true, // Default: true = padded layout (/pad toggles)
        }
    }
}

fn get_state_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config/rs-agent/state.json"))
        .unwrap_or_else(|| PathBuf::from("state.json"))
}

fn load_state() -> AppState {
    let path = get_state_path();
    fs::read_to_string(path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn save_state(state: &AppState) {
    let path = get_state_path();
    if let Ok(content) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, content);
    }
}

pub fn get_box_style() -> usize {
    let style = load_state().box_style;
    if (1..=6).contains(&style) {
        style
    } else {
        AppState::default().box_style
    }
}

pub fn set_box_style(style: usize) {
    let mut state = load_state();
    state.box_style = style;
    save_state(&state);
}

pub fn get_pad() -> bool {
    load_state().pad
}

pub fn set_pad(pad: bool) {
    let mut state = load_state();
    state.pad = pad;
    save_state(&state);
}
