use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct AppState {
    pub box_style: usize,
    #[serde(default)]
    pub pad: bool,
    #[serde(default, rename = "showThinkingSummaries", skip_serializing_if = "Option::is_none")]
    pub show_thinking_summaries: Option<bool>,
}

// =============================================================================
// THE SINGLE SOURCE FOR DEFAULTS:
// =============================================================================
impl Default for AppState {
    fn default() -> Self {
        Self {
            box_style: 7,
            pad: true, // Default: Box 7 spaced layout (/box 1-6 disables pad; /pad overrides)
            show_thinking_summaries: Some(true),
        }
    }
}

fn get_state_path() -> PathBuf {
    dirs::home_dir().map(|h| h.join(".config/rs-agent/state.json")).unwrap_or_else(|| PathBuf::from("state.json"))
}

fn load_state() -> AppState {
    let path = get_state_path();
    fs::read_to_string(path).ok().and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default()
}

fn save_state(state: &AppState) {
    let path = get_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, content);
    }
}

pub fn get_box_style() -> usize {
    let style = load_state().box_style;
    if (1..=7).contains(&style) {
        style
    } else {
        AppState::default().box_style
    }
}

fn apply_box_style(state: &mut AppState, style: usize) {
    state.box_style = style;
    state.pad = style == 7;
    state.show_thinking_summaries = if style == 7 { Some(true) } else { None };
}

pub fn set_box_style(style: usize) {
    let mut state = load_state();
    apply_box_style(&mut state, style);
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

pub fn get_show_thinking_summaries() -> Option<bool> {
    load_state().show_thinking_summaries
}

pub fn set_show_thinking_summaries(val: Option<bool>) {
    let mut state = load_state();
    state.show_thinking_summaries = val;
    save_state(&state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box7_and_compact_defaults() {
        let mut s = AppState::default();
        assert_eq!(s.box_style, 7);
        assert!(s.pad);
        assert_eq!(s.show_thinking_summaries, Some(true));

        apply_box_style(&mut s, 4);
        assert!(!s.pad);
        assert_eq!(s.show_thinking_summaries, None);

        apply_box_style(&mut s, 7);
        assert!(s.pad);
        assert_eq!(s.show_thinking_summaries, Some(true));
    }
}
