use crate::client::ChatMessage;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    if (1..=8).contains(&style) {
        style
    } else {
        AppState::default().box_style
    }
}

fn apply_box_style(state: &mut AppState, style: usize) {
    state.box_style = style;
    state.pad = style == 7 || style == 8;
    state.show_thinking_summaries = if style == 7 || style == 8 { Some(true) } else { None };
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

#[derive(Default, Serialize, Deserialize)]
pub struct WorkspaceSession {
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_history: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_stats: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

pub fn load_workspace_session(workspace: &Path) -> WorkspaceSession {
    fs::read_to_string(workspace.join(".rs/session.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_workspace_session(workspace: &Path, session: &WorkspaceSession) {
    let dir = workspace.join(".rs");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(content) = serde_json::to_string(session) else {
        return;
    };
    let final_path = dir.join("session.json");
    let tmp_path = dir.join(format!("session.json.tmp.{}", std::process::id()));
    if fs::write(&tmp_path, content).is_err() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
    }
    let _ = fs::rename(&tmp_path, &final_path);
}

pub fn clear_workspace_session(workspace: &Path) {
    let _ = fs::remove_file(workspace.join(".rs/session.json"));
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

        apply_box_style(&mut s, 8);
        assert!(s.pad);
        assert_eq!(s.show_thinking_summaries, Some(true));
    }

    #[test]
    fn test_workspace_session_roundtrip_and_clear() {
        let tmp = std::env::temp_dir().join(format!("rs_agent_sess_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        assert!(load_workspace_session(&tmp).messages.is_empty());

        let snap = WorkspaceSession {
            messages: vec![ChatMessage::user("hello"), ChatMessage::assistant("hi")],
            active_skills: vec!["ponytail".into()],
            reasoning: Some("250".into()),
            ..Default::default()
        };
        save_workspace_session(&tmp, &snap);
        let loaded = load_workspace_session(&tmp);
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content_as_str(), "hello");
        assert_eq!(loaded.active_skills, vec!["ponytail"]);
        assert_eq!(loaded.reasoning.as_deref(), Some("250"));

        clear_workspace_session(&tmp);
        assert!(load_workspace_session(&tmp).messages.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }
}
