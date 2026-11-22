use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};

use rs_agent::adapter;
use rs_agent::client::{is_local_endpoint, ChatMessage, LlmClient};
use rs_agent::config::{
    get_env_path, get_max_rounds, get_max_tokens, set_profile_max_tokens, strip_inline_comment, ProviderConfig,
    ToolTier, WorkspaceConfig,
};
use rs_agent::mcp::{self, McpServerConfig};
use rs_agent::profile::{build_system_prompt, Profile, ReasoningSetting};
use rs_agent::security::SecurityBoundary;
use rs_agent::skill::Skill;
use rs_agent::state;
use rs_agent::tools;
use rs_agent::ui::format_endpoint;

const UI_HTML: &str = include_str!("ui.html");

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct RailConfig {
    #[serde(default = "default_true")]
    show_right_panel: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_project: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hidden_projects: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    project_labels: HashMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    project_names: HashMap<String, String>,
}

impl Default for RailConfig {
    fn default() -> Self {
        Self {
            show_right_panel: true,
            last_project: None,
            hidden_projects: Vec::new(),
            project_labels: HashMap::new(),
            project_names: HashMap::new(),
        }
    }
}

impl RailConfig {
    fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> Result<()> {
        atomic_write(path, &serde_json::to_string_pretty(self)?)
    }
}

fn sanitize_label(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 24 || s.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(s.to_string())
}

fn sanitize_hidden_projects(raw: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = raw
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|s| is_valid_slug(s))
        .map(ToString::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

fn sanitize_project_labels(obj: &serde_json::Map<String, Value>) -> HashMap<String, Vec<String>> {
    let mut out = HashMap::new();
    for (proj, val) in obj {
        if !is_valid_slug(proj) {
            continue;
        }
        let mut labels = Vec::new();
        if let Some(arr) = val.as_array() {
            for item in arr {
                if let Some(clean) = item.as_str().and_then(sanitize_label) {
                    if !labels.contains(&clean) && labels.len() < 8 {
                        labels.push(clean);
                    }
                }
            }
        }
        if !labels.is_empty() {
            out.insert(proj.clone(), labels);
        }
    }
    out
}

fn sanitize_project_names(obj: &serde_json::Map<String, Value>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (proj, val) in obj {
        if !is_valid_slug(proj) {
            continue;
        }
        if let Some(raw) = val.as_str() {
            let s = raw.trim();
            if !s.is_empty() && s != proj && s.chars().count() <= 36 && !s.chars().any(|c| c.is_control()) {
                out.insert(proj.clone(), s.to_string());
            }
        }
    }
    out
}

fn find_profile_text_in(profiles_dir: &Path, profile_name: &str) -> Option<String> {
    let direct = profiles_dir.join(format!("{profile_name}.md"));
    if direct.is_file() {
        return fs::read_to_string(&direct).ok();
    }
    let stem = profile_name.split('/').next_back().unwrap_or(profile_name);
    let stem_norm = stem.replace(['-', '_'], "").to_ascii_lowercase();
    if let Ok(rd) = fs::read_dir(profiles_dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() && !ent.file_name().to_string_lossy().starts_with('.') {
                let sub = p.join(format!("{stem}.md"));
                if sub.is_file() {
                    return fs::read_to_string(&sub).ok();
                }
            } else if p.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(s) = p.file_stem().and_then(|x| x.to_str()) {
                    if s.replace(['-', '_'], "").to_ascii_lowercase() == stem_norm {
                        return fs::read_to_string(&p).ok();
                    }
                }
            }
        }
    }
    None
}

fn derive_auto_labels_in(root: &Path, project: &str, prof_name: &str) -> Vec<String> {
    if project == "chat" {
        return Vec::new();
    }
    let Some(raw) = find_profile_text_in(&root.join("profiles"), prof_name) else {
        return Vec::new();
    };
    let mut in_fm = false;
    for line in raw.lines() {
        let t = line.trim();
        if t == "---" {
            if !in_fm {
                in_fm = true;
                continue;
            }
            break;
        }
        if in_fm {
            if let Some((k, v)) = t.split_once(':') {
                if matches!(k.trim(), "label" | "labels") {
                    let clean = strip_inline_comment(v.trim()).trim_matches(['[', ']']);
                    let mut list = Vec::new();
                    for item in clean.split(',') {
                        if let Some(lbl) = sanitize_label(item.trim_matches(['"', '\'', ' '])) {
                            if !list.contains(&lbl) && list.len() < 8 {
                                list.push(lbl);
                            }
                        }
                    }
                    if !list.is_empty() {
                        return list;
                    }
                }
            }
        }
    }
    let prof = Profile::parse(&raw);
    if !prof.mcp_servers.is_empty() {
        vec!["mcp".into()]
    } else {
        match prof.tier {
            ToolTier::PureBash => vec!["purebash".into()],
            ToolTier::Core4 => vec!["cloud".into()],
            ToolTier::Smol => vec!["local".into()],
        }
    }
}

fn apply_auto_labels(root: &Path, cfg: &mut RailConfig) -> usize {
    let mut added = 0;
    let Ok(rd) = fs::read_dir(root.join("projects")) else {
        return 0;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if !p.is_dir() {
            continue;
        }
        let Some(name) = p.file_name().and_then(|n| n.to_str()).map(ToString::to_string) else {
            continue;
        };
        if name.starts_with('.') || name == "chat" {
            continue;
        }
        let allow_refresh = cfg
            .project_labels
            .get(&name)
            .map(|cur| !cur.is_empty() && cur.iter().all(|l| matches!(l.as_str(), "mcp" | "purebash" | "cloud" | "local")))
            .unwrap_or(true);
        if !allow_refresh {
            continue;
        }
        let ws_cfg = WorkspaceConfig::load_or_default(&p);
        let prof_name = if ws_cfg.profile_name == "chat" { name.as_str() } else { ws_cfg.profile_name.as_str() };
        let labels = derive_auto_labels_in(root, &name, prof_name);
        if !labels.is_empty() && cfg.project_labels.get(&name) != Some(&labels) {
            cfg.project_labels.insert(name, labels);
            added += 1;
        }
    }
    added
}

#[derive(Clone, Serialize, Deserialize)]
struct DiffLine {
    kind: String,
    num: usize,
    text: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct DiffPayload {
    adds: usize,
    rems: usize,
    lines: Vec<DiffLine>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct UiTurnItem {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    brief: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<DiffPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct TurnStats {
    prompt_tokens: usize,
    completion_tokens: usize,
    reasoning_tokens: usize,
    cached_tokens: usize,
    elapsed_s: f64,
    tps: f64,
}

struct SessionState {
    project_name: String,
    workspace: PathBuf,
    profile_name: String,
    is_agent: bool,
    adapters: bool,
    delegate: bool,
    tier: ToolTier,
    mcp_servers: Vec<String>,
    mcp_tools: Vec<String>,
    ctx_override: Option<usize>,
    hud_summary: String,
    reasoning: Option<ReasoningSetting>,
    active_skills: Vec<String>,
    llm_history: Vec<ChatMessage>,
    ui_history: Vec<UiTurnItem>,
    last_stats: Option<TurnStats>,
    cancel_flag: Arc<AtomicBool>,
    generation: u64,
}

struct AppRuntime {
    root: PathBuf,
    sessions: Mutex<HashMap<String, SessionState>>,
    active_project: Mutex<String>,
    pending_perms: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
    next_perm_id: AtomicU64,
    active_clients: AtomicUsize,
    client_seq: AtomicU64,
    http: reqwest::Client,
    noctalia_catalog: Mutex<Option<Vec<Value>>>,
    noctalia_registry: Mutex<HashMap<String, Value>>,
}

impl AppRuntime {
    fn new() -> Self {
        let root = dirs::home_dir()
            .map(|h| h.join(".config/rs-agent"))
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new_in(root)
    }

    fn new_in(root: PathBuf) -> Self {
        let rcfg = RailConfig::load(&root.join("plugins/rs-rail/config.json"));
        let initial = rcfg
            .last_project
            .filter(|p| is_valid_slug(p) && !rcfg.hidden_projects.contains(p) && root.join("projects").join(p).is_dir())
            .unwrap_or_else(|| "chat".into());
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .user_agent("rs-rail")
            .build()
            .unwrap_or_default();
        Self {
            root,
            sessions: Mutex::new(HashMap::new()),
            active_project: Mutex::new(initial),
            pending_perms: Mutex::new(HashMap::new()),
            next_perm_id: AtomicU64::new(1),
            active_clients: AtomicUsize::new(0),
            client_seq: AtomicU64::new(0),
            http,
            noctalia_catalog: Mutex::new(None),
            noctalia_registry: Mutex::new(HashMap::new()),
        }
    }

    fn load_or_init_session(&self, project: &str) -> SessionState {
        let proj_dir = self.root.join("projects").join(project);
        let _ = fs::create_dir_all(&proj_dir);
        let is_agent = project != "chat";
        let mut cfg = WorkspaceConfig::load_or_default(&proj_dir);
        if !is_agent {
            cfg.is_agent = false;
            cfg.profile_name = "chat".into();
        } else {
            cfg.is_agent = true;
            if cfg.profile_name == "chat" || Profile::load(&cfg.profile_name).is_none() {
                if Profile::load(project).is_some() {
                    cfg.profile_name = project.into();
                } else {
                    let avail = Profile::list_available();
                    let cand = Profile::find_workspace_candidates(project, &avail);
                    if let Some(first) = cand.first() {
                        cfg.profile_name = first.clone();
                    }
                }
            }
        }

        let saved = state::load_workspace_session(&proj_dir);
        let active_skills: Vec<String> = saved
            .active_skills
            .into_iter()
            .filter(|sk| Skill::load(sk).is_some())
            .take(3)
            .collect();
        let reasoning = match saved.reasoning {
            Some(ref r) if r.is_empty() => None,
            Some(ref r) => ReasoningSetting::parse(r),
            None => None,
        };

        let loaded_prof = Profile::load(&cfg.profile_name);
        let profile_missing = loaded_prof.is_none();
        let prof = loaded_prof.unwrap_or_else(|| Profile::parse("ROLE: Assistant."));
        if is_agent && !proj_dir.join(".rs/config.json").is_file() {
            cfg.tier = prof.tier.clone();
            cfg.adapters = prof.adapters;
            cfg.delegate = prof.delegate;
            let _ = cfg.save(&proj_dir);
        }
        // Omit profile label in header only when its .md file is missing on disk.
        if profile_missing {
            cfg.profile_name.clear();
        }
        let hud_summary = prof.hud_summary(cfg.is_agent);
        set_profile_max_tokens(prof.ctx);
        let sys = build_system_prompt(&prof, &proj_dir, cfg.is_agent, &active_skills);
        let final_reasoning = if reasoning.is_some() { reasoning } else if saved.reasoning.is_some() { None } else { prof.reasoning_budget };

        let mut llm_history = vec![ChatMessage::system(sys)];
        llm_history.extend(saved.messages.clone());
        let ui_history: Vec<UiTurnItem> = saved
            .ui_history
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_else(|| ui_items_from_messages(&saved.messages));
        let last_stats: Option<TurnStats> = saved.last_stats.and_then(|v| serde_json::from_value(v).ok());

        SessionState {
            project_name: project.into(),
            workspace: proj_dir,
            profile_name: cfg.profile_name,
            is_agent: cfg.is_agent,
            adapters: prof.adapters,
            delegate: prof.delegate,
            tier: prof.tier,
            mcp_servers: prof.mcp_servers,
            mcp_tools: prof.mcp_tools,
            ctx_override: prof.ctx,
            hud_summary,
            reasoning: final_reasoning,
            active_skills,
            llm_history,
            ui_history,
            last_stats,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            generation: 0,
        }
    }
}

fn is_protected_delete_path(rel: &str, target: &Path, root: &Path) -> bool {
    if rel.is_empty() || rel.split(['/', '\\']).any(|s| s == ".git" || s == ".env") {
        return true;
    }
    let is_core_rs = |p: &Path| p == Path::new(".rs/config.json") || p == Path::new(".rs/memory.md");
    if target.strip_prefix(root).is_ok_and(is_core_rs) {
        return true;
    }
    if let (Ok(canon_t), Ok(canon_r)) = (target.canonicalize(), root.canonicalize()) {
        if canon_t.strip_prefix(&canon_r).is_ok_and(is_core_rs) {
            return true;
        }
    }
    false
}

fn is_valid_slug(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && name != "."
        && name != ".."
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn url_decode(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let raw = s.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            if let Ok(hex) = std::str::from_utf8(&raw[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    bytes.push(b);
                    i += 3;
                    continue;
                }
            }
        } else if raw[i] == b'+' {
            bytes.push(b' ');
            i += 1;
            continue;
        }
        bytes.push(raw[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn git_status_brief(dir: &Path) -> (String, bool) {
    let mut cur = Some(dir);
    let mut in_git = false;
    while let Some(d) = cur {
        if d.join(".git").exists() {
            // Do not attribute the parent rs-agent repo's status to a project without its own .git.
            if d.join("projects").is_dir() && dir != d {
                break;
            }
            in_git = true;
            break;
        }
        cur = d.parent();
    }
    if !in_git {
        return (String::new(), false);
    }
    let Ok(out) = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["--no-optional-locks", "status", "--porcelain", "-b", "--no-renames"])
        .output()
    else {
        return ("git: [none]".into(), false);
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let head = lines.next().unwrap_or("").trim_start_matches("##").trim();
    let branch = head.split("...").next().unwrap_or(head).split_whitespace().next().unwrap_or("detached");
    let dirty = lines.filter(|l| !l.trim().is_empty()).count();
    let label = if dirty == 0 {
        format!("git: {branch} · clean")
    } else {
        format!("git: {branch} · {dirty} modified")
    };
    (label, dirty > 0)
}

fn compute_diff(old_text: &str, new_text: &str) -> DiffPayload {
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();
    let min_len = old_lines.len().min(new_lines.len());
    let mut prefix = 0;
    while prefix < min_len && old_lines[prefix] == new_lines[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < (min_len - prefix)
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let removed = &old_lines[prefix..old_lines.len() - suffix];
    let added = &new_lines[prefix..new_lines.len() - suffix];
    let mut lines = Vec::new();
    if prefix > 0 {
        lines.push(DiffLine { kind: "ctx".into(), num: prefix, text: old_lines[prefix - 1].to_string() });
    }
    for (i, l) in removed.iter().take(12).enumerate() {
        lines.push(DiffLine { kind: "del".into(), num: prefix + i + 1, text: (*l).to_string() });
    }
    for (i, l) in added.iter().take(12).enumerate() {
        lines.push(DiffLine { kind: "add".into(), num: prefix + i + 1, text: (*l).to_string() });
    }
    if suffix > 0 {
        let idx = old_lines.len() - suffix;
        lines.push(DiffLine { kind: "ctx".into(), num: idx + 1, text: old_lines[idx].to_string() });
    }
    DiffPayload { adds: added.len(), rems: removed.len(), lines }
}

fn extract_quoted_val(line: &str) -> &str {
    if let Some(pos) = line.find('=') {
        let rest = line[pos + 1..].trim();
        if let Some(s) = rest.strip_prefix('"') {
            if let Some(end) = s.find('"') {
                return &s[..end];
            }
        } else if let Some(s) = rest.strip_prefix('\'') {
            if let Some(end) = s.find('\'') {
                return &s[..end];
            }
        } else {
            return strip_inline_comment(rest);
        }
    }
    ""
}

#[derive(Serialize)]
struct EnvSlotView {
    id: String,
    name: String,
    model: String,
    is_active: bool,
}

#[inline]
fn is_env_key(line: &str, key: &str) -> bool {
    line.strip_prefix(key).is_some_and(|r| r.trim_start().starts_with('='))
}

#[inline]
fn is_valid_slot_val(val: &str) -> bool {
    !val.is_empty() && !val.contains("your-key-here") && !val.contains("your_key_here")
}

fn read_env_slots(env_path: &Path) -> (Vec<EnvSlotView>, usize, usize) {
    let raw = fs::read_to_string(env_path).unwrap_or_default();
    let lines: Vec<&str> = raw.lines().collect();
    let mut slots = Vec::new();

    for &(pfx, name, def_m) in &[
        ("GEMINI", "Google Gemini", "gemini-3.5-flash-lite"),
        ("GOOGLE", "Google AI", "gemini-3.5-flash-lite"),
        ("OPENROUTER", "OpenRouter", "openrouter/free"),
        ("OPENAI", "OpenAI", "gpt-4o"),
        ("ANTHROPIC", "Anthropic", "claude-3-5-sonnet-20241022"),
        ("DEEPSEEK", "DeepSeek", "deepseek-chat"),
        ("GROQ", "Groq", "llama-3.3-70b-versatile"),
    ] {
        let kv = format!("{pfx}_API_KEY");
        let mv = format!("{pfx}_MODEL");
        let mut found = false;
        let mut active = false;
        for l in &lines {
            let t = l.trim();
            let u = t.trim_start_matches('#').trim();
            if is_env_key(u, &kv) && is_valid_slot_val(extract_quoted_val(u)) {
                found = true;
                active = !t.starts_with('#');
                break;
            }
        }
        if found {
            let model = lines
                .iter()
                .find_map(|l| {
                    let t = l.trim();
                    (!t.starts_with('#') && is_env_key(t, &mv))
                        .then(|| extract_quoted_val(t).to_string())
                        .filter(|s| !s.is_empty())
                })
                .or_else(|| {
                    lines.iter().find_map(|l| {
                        let u = l.trim().trim_start_matches('#').trim();
                        is_env_key(u, &mv)
                            .then(|| extract_quoted_val(u).to_string())
                            .filter(|s| !s.is_empty())
                    })
                })
                .unwrap_or_else(|| def_m.into());
            slots.push(EnvSlotView { id: pfx.into(), name: name.into(), model, is_active: active });
        }
    }

    for i in 1..=9 {
        let kv = format!("CUSTOM{i}_API_KEY");
        let nv = format!("CUSTOM{i}_NAME");
        let mv = format!("CUSTOM{i}_MODEL");
        let mut found = false;
        let mut active = false;
        let mut name = format!("Custom {i}");
        let mut active_model: Option<String> = None;
        let mut fallback_model: Option<String> = None;
        for l in &lines {
            let t = l.trim();
            let u = t.trim_start_matches('#').trim();
            if is_env_key(u, &kv) && is_valid_slot_val(extract_quoted_val(u)) {
                found = true;
                active = !t.starts_with('#');
            } else if is_env_key(u, &nv) {
                let v = extract_quoted_val(u);
                if !v.is_empty() {
                    name = v.into();
                }
            } else if is_env_key(u, &mv) {
                let v = extract_quoted_val(u);
                if !v.is_empty() {
                    if !t.starts_with('#') {
                        active_model = Some(v.into());
                    } else if fallback_model.is_none() {
                        fallback_model = Some(v.into());
                    }
                }
            }
        }
        if found {
            let model = active_model.or(fallback_model).unwrap_or_else(|| "default".into());
            slots.push(EnvSlotView { id: format!("CUSTOM{i}"), name, model, is_active: active });
        }
    }

    let mut env_tok = 8192;
    let mut env_rnd = 15;
    for l in &lines {
        let t = l.trim();
        if !t.starts_with('#') && is_env_key(t, "AI_MAX_TOKENS") {
            if let Ok(v) = extract_quoted_val(t).parse() {
                env_tok = v;
            }
        } else if !t.starts_with('#') && is_env_key(t, "AI_MAX_AGENT_ROUNDS") {
            if let Ok(v) = extract_quoted_val(t).parse() {
                env_rnd = v;
            }
        }
    }
    (slots, env_tok, env_rnd)
}

fn update_env_file(
    env_path: &Path,
    toggle_slot: Option<&str>,
    max_tokens: Option<usize>,
    max_rounds: Option<usize>,
) -> Result<()> {
    if let Some(parent) = env_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let raw = fs::read_to_string(env_path).unwrap_or_default();
    let mut lines: Vec<String> = raw.lines().map(ToString::to_string).collect();

    if let Some(target) = toggle_slot {
        let target_key = format!("{target}_API_KEY");
        let was_active = lines.iter().any(|l| {
            let t = l.trim();
            !t.starts_with('#') && is_env_key(t, &target_key)
        });
        let activate = !was_active;

        let prefixes: Vec<String> = ["GEMINI", "GOOGLE", "OPENROUTER", "OPENAI", "ANTHROPIC", "DEEPSEEK", "GROQ"]
            .into_iter()
            .map(String::from)
            .chain((1..=9).map(|i| format!("CUSTOM{i}")))
            .collect();

        let is_custom_target = activate && target.starts_with("CUSTOM");
        for line in &mut lines {
            let trimmed = line.trim();
            let was_commented = trimmed.starts_with('#');
            let uncommented = trimmed.trim_start_matches('#').trim_start();
            let mut replacement: Option<String> = None;
            for pfx in &prefixes {
                let kv = format!("{pfx}_API_KEY");
                if is_env_key(uncommented, &kv) {
                    replacement = Some(if activate && pfx == target && is_valid_slot_val(extract_quoted_val(uncommented)) {
                        uncommented.to_string()
                    } else {
                        format!("#{uncommented}")
                    });
                    break;
                }
            }
            if replacement.is_none() && is_custom_target && was_commented {
                for suffix in ["_NAME", "_URL", "_MODEL"] {
                    if is_env_key(uncommented, &format!("{target}{suffix}"))
                        && is_valid_slot_val(extract_quoted_val(uncommented))
                    {
                        replacement = Some(uncommented.to_string());
                        break;
                    }
                }
            }
            if let Some(next) = replacement {
                *line = next;
            }
        }
    }

    for (key, val_opt) in [("AI_MAX_TOKENS", max_tokens), ("AI_MAX_AGENT_ROUNDS", max_rounds)] {
        if let Some(v) = val_opt {
            let mut found = false;
            for l in &mut lines {
                if is_env_key(l.trim().trim_start_matches('#').trim(), key) {
                    *l = format!("{key}=\"{v}\"");
                    found = true;
                    break;
                }
            }
            if !found {
                lines.push(format!("{key}=\"{v}\""));
            }
        }
    }

    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    let tmp = env_path.with_extension(format!("env.tmp.{}", std::process::id()));
    fs::write(&tmp, &out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, env_path)?;
    Ok(())
}

fn is_light_hex(hex: &str) -> bool {
    let h = hex.trim().trim_start_matches('#');
    if !h.is_ascii() || h.len() < 6 {
        return false;
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0) as u32;
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0) as u32;
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0) as u32;
    (r * 299 + g * 587 + b * 114) / 1000 > 140
}

fn parse_omarchy_toml(dir: &Path) -> Option<Value> {
    let raw = fs::read_to_string(dir.join("colors.toml")).ok()?;
    let mut map: HashMap<String, String> = HashMap::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let val = strip_inline_comment(v.trim()).trim_matches(['"', '\'']).trim();
            if !val.is_empty() {
                map.insert(k.trim().to_string(), val.to_string());
            }
        }
    }
    let getk = |keys: &[&str]| -> Option<String> { keys.iter().find_map(|k| map.get(*k)).cloned() };
    let panel = getk(&["background", "bg", "dark_background", "dark_bg"])?;
    let bg = getk(&["darker_background", "darker_bg", "dark_background", "dark_bg"]).unwrap_or_else(|| panel.clone());
    let card = getk(&["lighter_background", "lighter_bg"]).unwrap_or_else(|| panel.clone());
    let border = getk(&["selection", "selection_background", "muted", "color8"]).unwrap_or_else(|| "#333842".into());
    let text = getk(&["foreground", "fg", "bright_foreground", "bright_fg"]).unwrap_or_else(|| "#e5e7eb".into());
    let muted = getk(&["dark_foreground", "dark_fg", "muted", "color8"]).unwrap_or_else(|| "#8b92a0".into());
    let accent = getk(&["accent", "color4", "blue", "green", "color2"]).unwrap_or_else(|| "#22c55e".into());
    let cyan = getk(&["cyan", "color6", "blue", "color4"]).unwrap_or_else(|| accent.clone());
    let is_light = dir.join("light.mode").is_file()
        || map.get("mode").is_some_and(|m| m.eq_ignore_ascii_case("light"))
        || is_light_hex(&bg);
    Some(json!({
        "id": "omarchy", "name": "Omarchy", "source": "live",
        "mode": if is_light { "light" } else { "dark" },
        "bg": bg, "panel": panel, "card": card, "border": border,
        "text": text, "muted": muted, "accent": accent, "cyan": cyan
    }))
}


fn parse_theme_json_file(path: &Path) -> Option<Value> {
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    let raw = fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let bg = v.get("bg").and_then(|x| x.as_str())?;
    let accent = v.get("accent").and_then(|x| x.as_str())?;
    let panel = v.get("panel").and_then(|x| x.as_str()).unwrap_or(bg);
    let card = v.get("card").and_then(|x| x.as_str()).unwrap_or(panel);
    let border = v.get("border").and_then(|x| x.as_str()).unwrap_or("#2c303a");
    let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("#e5e7eb");
    let muted = v.get("muted").and_then(|x| x.as_str()).unwrap_or("#8b92a0");
    let cyan = v.get("cyan").and_then(|x| x.as_str()).unwrap_or(accent);
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or(stem);
    let mode = v.get("mode").and_then(|x| x.as_str()).unwrap_or(if is_light_hex(bg) { "light" } else { "dark" });
    let mut obj = json!({
        "id": format!("custom:{stem}"), "name": name, "source": "custom", "mode": mode,
        "bg": bg, "panel": panel, "card": card, "border": border,
        "text": text, "muted": muted, "accent": accent, "cyan": cyan
    });
    for extra in ["active_bg", "active_border", "user_bg", "user_border", "topbar_bg"] {
        if let Some(s) = v.get(extra).and_then(|x| x.as_str()) {
            obj[extra] = Value::String(s.to_string());
        }
    }
    if let Some(b) = v.get("semantic_diffs").and_then(|x| x.as_bool()) {
        obj["semantic_diffs"] = Value::Bool(b);
    }
    Some(obj)
}

fn load_themes(themes_dir: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/state"));
        let cfg_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
        let omarchy_t = parse_omarchy_toml(&state_home.join("omarchy/current/theme"))
            .or_else(|| parse_omarchy_toml(&cfg_home.join("omarchy/current/theme")));
        if let Some(t) = omarchy_t {
            out.push(t);
        }
        if let Some(t) = parse_noctalia_live_with_themes(themes_dir) {
            out.push(t);
        }
    }
    let mut files = Vec::new();
    if let Ok(rd) = fs::read_dir(themes_dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                files.push(p);
            }
        }
    }
    files.sort();
    for path in files {
        if let Some(obj) = parse_theme_json_file(&path) {
            out.push(obj);
        }
    }
    out
}

fn is_valid_hex(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7 && b[0] == b'#' && b[1..].iter().all(|c| c.is_ascii_hexdigit())
}

fn hex_rgb(s: &str) -> Option<(u8, u8, u8)> {
    if !is_valid_hex(s) {
        return None;
    }
    let r = u8::from_str_radix(&s[1..3], 16).ok()?;
    let g = u8::from_str_radix(&s[3..5], 16).ok()?;
    let b = u8::from_str_radix(&s[5..7], 16).ok()?;
    Some((r, g, b))
}

fn rel_lum(c: u8) -> f64 {
    let x = c as f64 / 255.0;
    if x <= 0.03928 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}

fn contrast_ratio(a: &str, b: &str) -> f64 {
    let (Some((r1, g1, b1)), Some((r2, g2, b2))) = (hex_rgb(a), hex_rgb(b)) else {
        return 1.0;
    };
    let l1 = 0.2126 * rel_lum(r1) + 0.7152 * rel_lum(g1) + 0.0722 * rel_lum(b1);
    let l2 = 0.2126 * rel_lum(r2) + 0.7152 * rel_lum(g2) + 0.0722 * rel_lum(b2);
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

fn blend_hex(a: &str, b: &str, t: f64) -> String {
    let (Some((r1, g1, b1)), Some((r2, g2, b2))) = (hex_rgb(a), hex_rgb(b)) else {
        return a.to_string();
    };
    let mix = |x: u8, y: u8| -> u8 { ((x as f64) * (1.0 - t) + (y as f64) * t).round().clamp(0.0, 255.0) as u8 };
    format!("#{:02x}{:02x}{:02x}", mix(r1, r2), mix(g1, g2), mix(b1, b2))
}

fn pick_hex<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(*k).and_then(|x| x.as_str())).filter(|s| is_valid_hex(s))
}

fn map_noctalia_variant(name: &str, mode: &str, variant: &Value) -> Option<Value> {
    let bg = pick_hex(variant, &["surface", "mSurface"])?;
    let card = pick_hex(variant, &["surface_container", "surfaceVariant", "mSurfaceVariant"]).unwrap_or(bg);
    let panel = pick_hex(variant, &["surface_container_low"])
        .map(ToString::to_string)
        .unwrap_or_else(|| blend_hex(bg, card, 0.5));
    let border = pick_hex(variant, &["outline_variant", "outline", "mOutline"]).unwrap_or("#2c303a");
    let text = pick_hex(variant, &["on_surface", "mOnSurface"]).unwrap_or("#e5e7eb");
    let muted = pick_hex(variant, &["on_surface_variant", "mOnSurfaceVariant"]).unwrap_or("#8b92a0");
    let accent = pick_hex(variant, &["primary", "mPrimary"])?;
    let sec = pick_hex(variant, &["secondary", "mSecondary"]);
    let ter = pick_hex(variant, &["tertiary", "mTertiary"]);
    let cyan = [sec, ter, Some(accent)]
        .into_iter()
        .flatten()
        .find(|c| contrast_ratio(c, bg) >= 3.0)
        .unwrap_or(accent);
    let actual_mode = if mode == "light" { "light" } else { "dark" };
    Some(json!({
        "name": name, "mode": actual_mode,
        "bg": bg, "panel": panel, "card": card, "border": border,
        "text": text, "muted": muted, "accent": accent, "cyan": cyan
    }))
}

#[derive(Clone, Copy)]
struct BuiltinColors {
    bg: &'static str,
    panel: &'static str,
    card: &'static str,
    border: &'static str,
    text: &'static str,
    muted: &'static str,
    accent: &'static str,
    cyan: &'static str,
}

impl BuiltinColors {
    fn to_variant_json(self) -> Value {
        json!({
            "surface": self.bg, "surface_container_low": self.panel, "surface_container": self.card,
            "outline_variant": self.border, "on_surface": self.text, "on_surface_variant": self.muted,
            "primary": self.accent, "secondary": self.cyan
        })
    }

    fn preview(self) -> Value {
        json!({ "bg": self.bg, "accent": self.accent, "cyan": self.cyan })
    }
}

struct BuiltinPalette {
    name: &'static str,
    dark: BuiltinColors,
    light: BuiltinColors,
}

const BUILTIN_NOCTALIA: &[BuiltinPalette] = &[
    BuiltinPalette {
        name: "Noctalia",
        dark: BuiltinColors { bg: "#070722", panel: "#0c0c27", card: "#11112d", border: "#21215f", text: "#f3edf7", muted: "#7c80b4", accent: "#fff59b", cyan: "#a9aefe" },
        light: BuiltinColors { bg: "#f3edf7", panel: "#eae3f2", card: "#e1d9ec", border: "#c5bdd6", text: "#070722", muted: "#4b4e7d", accent: "#3d449b", cyan: "#5960b8" },
    },
    BuiltinPalette {
        name: "Ayu",
        dark: BuiltinColors { bg: "#0b0e14", panel: "#131720", card: "#1e222a", border: "#3d424d", text: "#d1d1c7", muted: "#8e959e", accent: "#ffb454", cyan: "#39bae6" },
        light: BuiltinColors { bg: "#f8f9fa", panel: "#eff1f3", card: "#e4e6e9", border: "#c9cdd2", text: "#42474c", muted: "#6e757c", accent: "#ff8f40", cyan: "#55b4d4" },
    },
    BuiltinPalette {
        name: "Catppuccin",
        dark: BuiltinColors { bg: "#1e1e2e", panel: "#272839", card: "#313244", border: "#585b70", text: "#cdd6f4", muted: "#a6adc8", accent: "#cba6f7", cyan: "#89b4fa" },
        light: BuiltinColors { bg: "#eff1f5", panel: "#e6e9ef", card: "#ccd0da", border: "#9ca0b0", text: "#4c4f69", muted: "#6c6f85", accent: "#8839ef", cyan: "#1e66f5" },
    },
    BuiltinPalette {
        name: "Dracula",
        dark: BuiltinColors { bg: "#282a36", panel: "#313443", card: "#44475a", border: "#6272a4", text: "#f8f8f2", muted: "#b6bfd9", accent: "#bd93f9", cyan: "#8be9fd" },
        light: BuiltinColors { bg: "#fffbeb", panel: "#f4efe0", card: "#e8e3d5", border: "#cfcfde", text: "#1f1f1f", muted: "#6c664b", accent: "#644ac9", cyan: "#036a96" },
    },
    BuiltinPalette {
        name: "Eldritch",
        dark: BuiltinColors { bg: "#212337", panel: "#292b40", card: "#323449", border: "#454759", text: "#ebfafa", muted: "#a4b1cd", accent: "#37f499", cyan: "#04d1f9" },
        light: BuiltinColors { bg: "#eef7f7", panel: "#e2eeee", card: "#d5e4e4", border: "#b3c8c8", text: "#212337", muted: "#454759", accent: "#128a56", cyan: "#027a96" },
    },
    BuiltinPalette {
        name: "Gruvbox",
        dark: BuiltinColors { bg: "#282828", panel: "#2d2c2c", card: "#32302f", border: "#504945", text: "#ebdbb2", muted: "#a89984", accent: "#a9b665", cyan: "#7db2ba" },
        light: BuiltinColors { bg: "#fbf1c7", panel: "#f2e5bc", card: "#ebdbb2", border: "#bdae93", text: "#3c3836", muted: "#665c54", accent: "#79740e", cyan: "#427b58" },
    },
    BuiltinPalette {
        name: "Kanagawa",
        dark: BuiltinColors { bg: "#1f1f28", panel: "#252530", card: "#2a2a37", border: "#54546d", text: "#dcd7ba", muted: "#9cabca", accent: "#7e9cd8", cyan: "#7fb4ca" },
        light: BuiltinColors { bg: "#f2ecbc", panel: "#e7e1b1", card: "#dcd5a5", border: "#b5a67b", text: "#545464", muted: "#716e61", accent: "#4d699b", cyan: "#597b75" },
    },
    BuiltinPalette {
        name: "Nord",
        dark: BuiltinColors { bg: "#2e3440", panel: "#353b49", card: "#3b4252", border: "#4c566a", text: "#eceff4", muted: "#d8dee9", accent: "#88c0d0", cyan: "#81a1c1" },
        light: BuiltinColors { bg: "#eceff4", panel: "#e5e9f0", card: "#d8dee9", border: "#b8c2d1", text: "#2e3440", muted: "#4c566a", accent: "#5e81ac", cyan: "#4c7b8b" },
    },
    BuiltinPalette {
        name: "Rosé Pine",
        dark: BuiltinColors { bg: "#191724", panel: "#1f1d2e", card: "#26233a", border: "#403d52", text: "#e0def4", muted: "#908caa", accent: "#ebbcba", cyan: "#9ccfd8" },
        light: BuiltinColors { bg: "#faf4ed", panel: "#fffaf3", card: "#f2e9e1", border: "#dfdad9", text: "#575279", muted: "#797593", accent: "#d7827e", cyan: "#56949f" },
    },
    BuiltinPalette {
        name: "Tokyo-Night",
        dark: BuiltinColors { bg: "#1a1b26", panel: "#1f2335", card: "#24283b", border: "#414868", text: "#c0caf5", muted: "#a9b1d6", accent: "#7aa2f7", cyan: "#7dcfff" },
        light: BuiltinColors { bg: "#e1e2e7", panel: "#d5d6db", card: "#c8c9d0", border: "#a8aecb", text: "#343b58", muted: "#6172b0", accent: "#2e7de9", cyan: "#007197" },
    },
];

fn norm_palette_key(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => Some('e'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

fn read_toml_theme_section(path: &Path, map: &mut HashMap<String, String>) -> bool {
    let Ok(raw) = fs::read_to_string(path) else { return false };
    let mut in_theme = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') {
            in_theme = t == "[theme]";
            continue;
        }
        if in_theme {
            if let Some((k, v)) = t.split_once('=') {
                let val = strip_inline_comment(v.trim()).trim_matches(['"', '\'']).trim();
                if !val.is_empty() {
                    map.insert(k.trim().to_string(), val.to_string());
                }
            }
        }
    }
    true
}

fn read_noctalia_palette_file(path: &Path, mode: &str) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let variant = v.get(mode).or_else(|| v.get("dark")).unwrap_or(&v);
    let m = if v.get(mode).is_some() {
        mode
    } else if pick_hex(variant, &["surface", "mSurface"]).is_some_and(is_light_hex) {
        "light"
    } else {
        "dark"
    };
    map_noctalia_variant("Noctalia", m, variant)
}

fn as_live_noctalia(mut theme: Value) -> Value {
    theme["id"] = json!("noctalia");
    theme["source"] = json!("live");
    theme
}

fn parse_noctalia_live_in_with_themes(cfg_dir: &Path, state_dir: &Path, themes_dir: &Path) -> Option<Value> {
    parse_noctalia_live_raw_in(cfg_dir, state_dir, Some(themes_dir)).map(as_live_noctalia)
}

fn parse_noctalia_live_raw_in(cfg_dir: &Path, state_dir: &Path, themes_dir: Option<&Path>) -> Option<Value> {
    // 1. Direct colors.json (Noctalia v4 or v5 user template)
    if let Some(theme) = read_noctalia_palette_file(&cfg_dir.join("colors.json"), "dark") {
        return Some(theme);
    }

    // 2. Noctalia v5 TOML config + state override
    let mut map = HashMap::new();
    let mut found = read_toml_theme_section(&cfg_dir.join("config.toml"), &mut map);
    found |= read_toml_theme_section(&cfg_dir.join("settings.toml"), &mut map);
    found |= read_toml_theme_section(&state_dir.join("settings.toml"), &mut map);
    if !found {
        return None;
    }

    let mode = if map.get("mode").is_some_and(|m| m.eq_ignore_ascii_case("light")) { "light" } else { "dark" };
    let source = map.get("source").map(String::as_str).unwrap_or("builtin");

    if source == "wallpaper" {
        if let Some(td) = themes_dir {
            if let Some(t) = parse_theme_json_file(&td.join("noctalia.json")) {
                return Some(t);
            }
        }
    } else if source == "community" {
        if let Some(name) = map.get("community_palette") {
            let p1 = state_dir.join("community-palettes").join(format!("{name}.json"));
            let p2 = state_dir.join("community-palettes").join(name).join(format!("{name}.json"));
            if let Some(t) = read_noctalia_palette_file(&p1, mode).or_else(|| read_noctalia_palette_file(&p2, mode)) {
                return Some(t);
            }
        }
    } else if source == "custom" {
        if let Some(name) = map.get("custom_palette") {
            let p = cfg_dir.join("palettes").join(format!("{name}.json"));
            if let Some(t) = read_noctalia_palette_file(&p, mode) {
                return Some(t);
            }
        }
    }

    let wanted = map.get("builtin_palette").or_else(|| map.get("builtin")).map(String::as_str).unwrap_or("Noctalia");
    let wanted_k = norm_palette_key(wanted);
    let b = BUILTIN_NOCTALIA
        .iter()
        .find(|p| norm_palette_key(p.name) == wanted_k)
        .unwrap_or(&BUILTIN_NOCTALIA[0]);
    let v = if mode == "light" { b.light.to_variant_json() } else { b.dark.to_variant_json() };
    map_noctalia_variant("Noctalia", mode, &v)
}

fn parse_noctalia_live_with_themes(themes_dir: &Path) -> Option<Value> {
    let home = dirs::home_dir()?;
    let cfg_dir = std::env::var_os("NOCTALIA_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("noctalia")))
        .unwrap_or_else(|| home.join(".config/noctalia"));
    let state_dir = std::env::var_os("NOCTALIA_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(|p| PathBuf::from(p).join("noctalia")))
        .unwrap_or_else(|| home.join(".local/state/noctalia"));
    parse_noctalia_live_in_with_themes(&cfg_dir, &state_dir, themes_dir)
}

fn url_encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn palette_slug(name: &str, mode: &str) -> String {
    let mut base = String::new();
    let mut prev_dash = true;
    for ch in name.chars() {
        let mapped = match ch {
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => Some('e'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        };
        if let Some(c) = mapped {
            base.push(c);
            prev_dash = false;
        } else if !prev_dash {
            base.push('-');
            prev_dash = true;
        }
    }
    while base.ends_with('-') {
        base.pop();
    }
    if base.len() > 40 {
        base.truncate(40);
        while base.ends_with('-') {
            base.pop();
        }
    }
    if base.is_empty() {
        base = "theme".into();
    }
    if mode == "light" { format!("{base}-light") } else { base }
}

fn preview_from_variant(v: &Value) -> Value {
    let bg = pick_hex(v, &["surface", "mSurface"]).unwrap_or("#141416");
    let accent = pick_hex(v, &["primary", "mPrimary"]).unwrap_or("#38bdf8");
    let cyan = pick_hex(v, &["secondary", "mSecondary", "tertiary", "mTertiary"]).unwrap_or(accent);
    json!({ "bg": bg, "accent": accent, "cyan": cyan })
}

async fn fetch_json_ok(http: &reqwest::Client, url: &str) -> Option<Value> {
    let resp = http.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<Value>().await.ok()
}

fn builtin_noctalia_list() -> Vec<Value> {
    BUILTIN_NOCTALIA
        .iter()
        .map(|b| {
            json!({
                "name": b.name,
                "base": "Core",
                "core": true,
                "slug_dark": palette_slug(b.name, "dark"),
                "slug_light": palette_slug(b.name, "light"),
                "dark": b.dark.preview(),
                "light": b.light.preview()
            })
        })
        .collect()
}

async fn fetch_community_catalog(app: &AppRuntime, force: bool) -> (Vec<Value>, Option<String>) {
    if !force {
        if let Some(ref cached) = *app.noctalia_catalog.lock().await {
            return (cached.clone(), None);
        }
    }

    let remote = match fetch_json_ok(&app.http, "https://api.noctalia.dev/palettes").await {
        Some(v) => Some(v),
        None => fetch_json_ok(&app.http, "https://raw.githubusercontent.com/noctalia-dev/community-palettes/main/registry.json").await,
    };

    let Some(val) = remote else {
        return (Vec::new(), Some("Offline: could not load community palettes".into()));
    };

    let mut reg_map = HashMap::new();
    let arr_opt = val.as_array().or_else(|| val.get("themes").and_then(|t| t.as_array()));
    let mut community = Vec::new();
    if let Some(items) = arr_opt {
        for item in items {
            let Some(name) = item.get("name").and_then(|x| x.as_str()).filter(|s| !s.is_empty()) else {
                continue;
            };
            if (item.pointer("/dark/mSurface").is_some() || item.pointer("/dark/surface").is_some())
                && (item.pointer("/dark/mOutline").is_some() || item.pointer("/dark/outline_variant").is_some())
            {
                reg_map.insert(name.to_string(), item.clone());
            }
            let base = item.get("base").and_then(|x| x.as_str()).unwrap_or("Community");
            let dark = item.get("dark").map(preview_from_variant).unwrap_or_else(|| json!({}));
            let light = item.get("light").map(preview_from_variant).unwrap_or_else(|| json!({}));
            community.push(json!({
                "name": name,
                "base": base,
                "core": false,
                "slug_dark": palette_slug(name, "dark"),
                "slug_light": palette_slug(name, "light"),
                "dark": dark,
                "light": light
            }));
        }
    }
    community.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    if !reg_map.is_empty() {
        *app.noctalia_registry.lock().await = reg_map;
    }
    *app.noctalia_catalog.lock().await = Some(community.clone());
    (community, None)
}

async fn install_noctalia_theme(app: &AppRuntime, name: &str, mode: &str) -> Result<Value> {
    let name = name.trim();
    if name.is_empty() || name.len() > 80 {
        anyhow::bail!("Invalid palette name");
    }
    let norm_mode = if mode.eq_ignore_ascii_case("light") { "light" } else { "dark" };

    let variant_val = if let Some(b) = BUILTIN_NOCTALIA.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
        if norm_mode == "light" { b.light.to_variant_json() } else { b.dark.to_variant_json() }
    } else {
        let enc = url_encode_segment(name);
        let cached = app.noctalia_registry.lock().await.get(name).cloned();
        let palette = match cached {
            Some(v) => Some(v),
            None => {
                let u1 = format!("https://api.noctalia.dev/palette/{enc}");
                let u2 = format!("https://raw.githubusercontent.com/noctalia-dev/community-palettes/main/{enc}/{enc}.json");
                match fetch_json_ok(&app.http, &u1).await {
                    Some(v) => Some(v),
                    None => match fetch_json_ok(&app.http, &u2).await {
                        Some(v) => Some(v),
                        None => {
                            let reg = fetch_json_ok(&app.http, "https://raw.githubusercontent.com/noctalia-dev/community-palettes/main/registry.json").await;
                            reg.and_then(|r| {
                                r.get("themes")?
                                    .as_array()?
                                    .iter()
                                    .find(|t| t.get("name").and_then(|x| x.as_str()) == Some(name))
                                    .cloned()
                            })
                        }
                    },
                }
            }
        }
        .ok_or_else(|| anyhow::anyhow!("Could not fetch palette '{name}'"))?;

        palette
            .get(norm_mode)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Palette '{name}' has no {norm_mode} variant"))?
    };

    let theme = map_noctalia_variant(name, norm_mode, &variant_val)
        .ok_or_else(|| anyhow::anyhow!("Palette '{name}' missing required colors"))?;
    let slug = palette_slug(name, norm_mode);
    if !is_valid_slug(&slug) {
        anyhow::bail!("Invalid generated theme slug");
    }
    let path = app.root.join("plugins/rs-rail/themes").join(format!("{slug}.json"));
    atomic_write(&path, &(serde_json::to_string_pretty(&theme)? + "\n"))?;
    Ok(json!({ "id": format!("custom:{slug}"), "slug": slug, "theme": theme }))
}

fn delete_custom_theme(themes_dir: &Path, id: &str) -> Result<()> {
    let stem = id
        .strip_prefix("custom:")
        .ok_or_else(|| anyhow::anyhow!("Only custom themes can be deleted"))?;
    if !is_valid_slug(stem) {
        anyhow::bail!("Invalid theme id");
    }
    let themes_canon = themes_dir.canonicalize()?;
    let target = themes_dir.join(format!("{stem}.json"));
    let meta = fs::symlink_metadata(&target)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        anyhow::bail!("Not a regular theme file");
    }
    if !target.canonicalize()?.starts_with(&themes_canon) {
        anyhow::bail!("Path outside themes directory");
    }
    fs::remove_file(&target)?;
    Ok(())
}

fn list_workspace_files(dir: &Path, root: &Path, include_rs: bool, out: &mut Vec<String>) {
    if out.len() >= 120 {
        return;
    }
    if let Ok(rd) = fs::read_dir(dir) {
        let mut ents: Vec<_> = rd.flatten().collect();
        ents.sort_by_key(|e| e.file_name());
        for ent in ents {
            if out.len() >= 120 {
                break;
            }
            let name = ent.file_name();
            let s = name.to_string_lossy();
            let keep_dot = include_rs && dir == root && s == ".rs";
            if (!keep_dot && s.starts_with('.')) || s == "target" || s == "node_modules" {
                continue;
            }
            let p = ent.path();
            let Ok(meta) = fs::symlink_metadata(&p) else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                list_workspace_files(&p, root, include_rs, out);
            } else if meta.is_file() {
                if let Ok(rel) = p.strip_prefix(root) {
                    out.push(rel.to_string_lossy().into_owned());
                }
            }
        }
    }
}

fn clean_project_dir(proj_dir: &Path) -> Result<usize> {
    let mut removed = 0;
    if let Ok(rd) = fs::read_dir(proj_dir) {
        for ent in rd.flatten() {
            let s = ent.file_name().to_string_lossy().into_owned();
            if s == ".rs" || s == ".git" {
                continue;
            }
            let p = ent.path();
            let meta = fs::symlink_metadata(&p)?;
            if meta.is_dir() && !meta.file_type().is_symlink() {
                fs::remove_dir_all(&p)?;
            } else {
                fs::remove_file(&p)?;
            }
            removed += 1;
        }
    }
    Ok(removed)
}

fn export_session_markdown(sess: &SessionState) -> Result<String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let filename = format!("chat_{ts}.md");
    let file_path = sess.workspace.join(".rs/transcripts").join(&filename);
    let mut md = format!("# rs-rail Transcript — {}\n\n", sess.project_name);
    for item in &sess.ui_history {
        match item.role.as_str() {
            "user" => {
                let text = item.content.as_deref().unwrap_or("");
                md.push_str(&format!("**User:**\n\n{text}\n\n---\n\n"));
            }
            "assistant" => {
                if let Some(ref r) = item.reasoning {
                    md.push_str(&format!("> *Thinking:* {}\n\n", r.replace('\n', "\n> ")));
                }
                if let Some(ref text) = item.content {
                    md.push_str(&format!("**Assistant:**\n\n{text}\n\n---\n\n"));
                }
            }
            "tool_card" => {
                let name = item.name.as_deref().unwrap_or("tool");
                let brief = item.brief.as_deref().unwrap_or("");
                let sum = item.summary.as_deref().unwrap_or("Done");
                md.push_str(&format!("- `{name}({brief})` → {sum}\n\n"));
            }
            _ => {}
        }
    }
    atomic_write(&file_path, &md)?;
    Ok(format!(".rs/transcripts/{filename}"))
}

fn ui_items_from_messages(msgs: &[ChatMessage]) -> Vec<UiTurnItem> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < msgs.len() {
        let m = &msgs[i];
        match m.role.as_str() {
            "user" => {
                let (content, images) = match &m.content {
                    Value::String(s) => (if s.is_empty() { None } else { Some(s.clone()) }, None),
                    Value::Array(parts) => {
                        let mut txt = String::new();
                        let mut imgs = Vec::new();
                        for p in parts {
                            if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
                                txt.push_str(t);
                            } else if let Some(u) = p.pointer("/image_url/url").and_then(|v| v.as_str()) {
                                imgs.push(u.to_string());
                            }
                        }
                        (if txt.is_empty() { None } else { Some(txt) }, if imgs.is_empty() { None } else { Some(imgs) })
                    }
                    _ => (None, None),
                };
                out.push(UiTurnItem { role: "user".into(), content, images, ..Default::default() });
            }
            "assistant" => {
                let txt = m.content_as_str();
                if !txt.is_empty() || m.reasoning_content.is_some() {
                    out.push(UiTurnItem {
                        role: "assistant".into(),
                        content: if txt.is_empty() { None } else { Some(txt.to_string()) },
                        reasoning: m.reasoning_content.clone(),
                        ..Default::default()
                    });
                }
                if let Some(calls) = m.tool_calls.as_ref().and_then(|v| v.as_array()) {
                    let mut tool_outputs: HashMap<&str, &str> = HashMap::new();
                    let mut j = i + 1;
                    while j < msgs.len() && msgs[j].role == "tool" {
                        if let Some(id) = msgs[j].tool_call_id.as_deref() {
                            tool_outputs.insert(id, msgs[j].content_as_str());
                        }
                        j += 1;
                    }
                    for c in calls {
                        let cid = c["id"].as_str().unwrap_or("");
                        let cname = c["function"]["name"].as_str().unwrap_or("tool");
                        let raw_a = c["function"]["arguments"].as_str().unwrap_or("{}");
                        let brief = serde_json::from_str::<Value>(raw_a)
                            .ok()
                            .and_then(|v| v.get("path").or_else(|| v.get("command")).or_else(|| v.get("pattern")).and_then(|x| x.as_str()).map(ToString::to_string))
                            .unwrap_or_default();
                        let res_txt = tool_outputs.get(cid).copied().unwrap_or("");
                        let ok = !res_txt.starts_with("[tool error]") && !res_txt.starts_with("[denied]");
                        let summary = res_txt.lines().next().unwrap_or("Done").trim().to_string();
                        out.push(UiTurnItem {
                            role: "tool_card".into(),
                            name: Some(cname.into()),
                            brief: Some(brief),
                            ok: Some(ok),
                            summary: Some(summary),
                            raw: if res_txt.is_empty() { None } else { Some(res_txt.to_string()) },
                            ..Default::default()
                        });
                    }
                    i = j;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn save_rail_session(sess: &SessionState) {
    let r_str = sess.reasoning.as_ref().map(|r| match r {
        ReasoningSetting::Tokens(n) => n.to_string(),
        ReasoningSetting::Tier(t) => t.clone(),
    });
    let snap = state::WorkspaceSession {
        messages: sess.llm_history.iter().skip(1).cloned().collect(),
        ui_history: serde_json::to_value(&sess.ui_history).ok(),
        last_stats: sess.last_stats.as_ref().and_then(|s| serde_json::to_value(s).ok()),
        active_skills: sess.active_skills.clone(),
        reasoning: Some(r_str.unwrap_or_default()),
    };
    state::save_workspace_session(&sess.workspace, &snap);
}

fn set_last_project(root: &Path, name: &str) {
    let cfg_path = root.join("plugins/rs-rail/config.json");
    let mut cfg = RailConfig::load(&cfg_path);
    if cfg.last_project.as_deref() != Some(name) {
        cfg.last_project = Some(name.to_string());
        let _ = cfg.save(&cfg_path);
    }
}

fn build_user_chat_message(text: &str, images: &[String]) -> ChatMessage {
    if images.is_empty() {
        return ChatMessage::user(text);
    }
    let mut parts = Vec::with_capacity(images.len() + 1);
    if !text.is_empty() {
        parts.push(json!({ "type": "text", "text": text }));
    }
    for url in images {
        parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
    }
    ChatMessage {
        role: "user".into(),
        content: Value::Array(parts),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
    }
}

async fn write_http_json(stream: &mut TcpStream, status: u16, body: &Value) -> Result<()> {
    let txt = serde_json::to_string(body)?;
    let hdr = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        txt.len()
    );
    stream.write_all(hdr.as_bytes()).await?;
    stream.write_all(txt.as_bytes()).await?;
    Ok(())
}

async fn write_http_err(stream: &mut TcpStream, status: u16, msg: &str) -> Result<()> {
    let hdr = format!(
        "HTTP/1.1 {status} Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        msg.len()
    );
    stream.write_all(hdr.as_bytes()).await?;
    stream.write_all(msg.as_bytes()).await?;
    Ok(())
}

fn is_local_host(h: &str) -> bool {
    matches!(h, "127.0.0.1" | "localhost" | "[::1]")
        || h.starts_with("127.0.0.1:")
        || h.starts_with("localhost:")
        || h.starts_with("[::1]:")
}

#[inline]
fn should_shutdown(active_clients: usize, cur_seq: u64, seq_at_drop: u64) -> bool {
    active_clients == 0 && cur_seq == seq_at_drop
}

fn is_local_origin(o: &str) -> bool {
    matches!(o, "http://127.0.0.1" | "http://localhost" | "http://[::1]")
        || o.starts_with("http://127.0.0.1:")
        || o.starts_with("http://localhost:")
        || o.starts_with("http://[::1]:")
}

async fn handle_client(mut stream: TcpStream, app: Arc<AppRuntime>) -> Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 2048];
    let hdr_end;

    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            hdr_end = pos + 4;
            break;
        }
        if buf.len() > 65536 {
            return Ok(());
        }
    }

    let hdr_str = String::from_utf8_lossy(&buf[..hdr_end]);
    let mut lines = hdr_str.lines();
    let req_line = lines.next().unwrap_or("");
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let raw_target = parts.next().unwrap_or("/");
    let (path, query) = raw_target.split_once('?').unwrap_or((raw_target, ""));

    let mut content_len = 0usize;
    let mut host_ok = true;
    let mut origin_ok = true;
    let mut fetch_site_ok = true;
    let mut is_json_ct = false;
    for l in lines {
        let low = l.to_ascii_lowercase();
        if let Some(v) = low.strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = low.strip_prefix("host:") {
            host_ok = is_local_host(v.trim());
        } else if let Some(v) = low.strip_prefix("origin:") {
            origin_ok = is_local_origin(v.trim());
        } else if let Some(v) = low.strip_prefix("sec-fetch-site:") {
            let s = v.trim();
            fetch_site_ok = s == "same-origin" || s == "none";
        } else if let Some(v) = low.strip_prefix("content-type:") {
            is_json_ct = v.trim().starts_with("application/json");
        }
    }
    if !host_ok || !origin_ok || !fetch_site_ok {
        return write_http_err(&mut stream, 403, "Forbidden origin/host").await;
    }
    if content_len > 16 * 1024 * 1024 {
        return write_http_err(&mut stream, 413, "Payload too large").await;
    }

    let mut body = buf[hdr_end..].to_vec();
    while body.len() < content_len {
        let need = (content_len - body.len()).min(tmp.len());
        let n = stream.read(&mut tmp[..need]).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_len);

    if method == "GET" && path == "/" {
        app.client_seq.fetch_add(1, Ordering::SeqCst);
        let hdr = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            UI_HTML.len()
        );
        stream.write_all(hdr.as_bytes()).await?;
        stream.write_all(UI_HTML.as_bytes()).await?;
        return Ok(());
    }

    if method == "GET" && path == "/api/health" {
        return write_http_json(&mut stream, 200, &json!({ "status": "ok" })).await;
    }

    if method == "GET" && path == "/api/live" {
        let hdr = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n: ok\n\n";
        stream.write_all(hdr.as_bytes()).await?;
        stream.flush().await?;

        app.active_clients.fetch_add(1, Ordering::SeqCst);
        app.client_seq.fetch_add(1, Ordering::SeqCst);

        let mut ping = tokio::time::interval(std::time::Duration::from_secs(25));
        ping.tick().await;
        loop {
            tokio::select! {
                res = stream.read(&mut tmp) => {
                    match res {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
                _ = ping.tick() => {
                    if stream.write_all(b":\n\n").await.is_err() || stream.flush().await.is_err() {
                        break;
                    }
                }
            }
        }

        let seq_at_drop = app.client_seq.load(Ordering::SeqCst);
        if app.active_clients.fetch_sub(1, Ordering::SeqCst) == 1 {
            let app_exit = Arc::clone(&app);
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if should_shutdown(
                    app_exit.active_clients.load(Ordering::SeqCst),
                    app_exit.client_seq.load(Ordering::SeqCst),
                    seq_at_drop,
                ) {
                    std::process::exit(0);
                }
            });
        }
        return Ok(());
    }

    if method == "GET" && path == "/api/themes" {
        return write_http_json(&mut stream, 200, &json!({ "themes": load_themes(&app.root.join("plugins/rs-rail/themes")) })).await;
    }

    if method == "GET" && path == "/api/noctalia/catalog" {
        let force = query.split('&').any(|p| p == "refresh=1");
        let scope = query.split('&').find_map(|p| p.strip_prefix("scope=")).unwrap_or("all");
        let (palettes, warning) = match scope {
            "core" => (builtin_noctalia_list(), None),
            "community" => fetch_community_catalog(&app, force).await,
            _ => {
                let mut all = builtin_noctalia_list();
                let (comm, warn) = fetch_community_catalog(&app, force).await;
                all.extend(comm);
                (all, warn)
            }
        };
        return write_http_json(&mut stream, 200, &json!({ "palettes": palettes, "warning": warning })).await;
    }


    if method == "GET" && path == "/api/file" {
        let rel = query.split('&').find_map(|p| p.strip_prefix("path=")).map(url_decode).unwrap_or_default();
        let active = app.active_project.lock().await.clone();
        let proj_dir = app.root.join("projects").join(&active);
        let bnd = SecurityBoundary::new_headless(&proj_dir);
        let target = bnd.resolve_path(&rel);
        if rel.is_empty() || bnd.is_outside(&target) || rel.split(['/', '\\']).any(|s| s == ".git" || s == ".env") {
            return write_http_err(&mut stream, 403, "Path out of bounds").await;
        }
        let Ok(meta) = fs::symlink_metadata(&target) else {
            return write_http_err(&mut stream, 404, "File not found").await;
        };
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 512 * 1024 {
            return write_http_err(&mut stream, 400, "Cannot preview file").await;
        }
        let bytes = fs::read(&target)?;
        let Ok(content) = String::from_utf8(bytes) else {
            return write_http_err(&mut stream, 400, "Binary file cannot be previewed").await;
        };
        return write_http_json(&mut stream, 200, &json!({ "path": rel, "content": content })).await;
    }

    if method == "GET" && path == "/api/state" {
        if let Some(sel) = query.split('&').find_map(|p| p.strip_prefix("project=")).map(url_decode).filter(|s| !s.is_empty()) {
            if is_valid_slug(&sel) && app.root.join("projects").join(&sel).is_dir() {
                set_last_project(&app.root, &sel);
                *app.active_project.lock().await = sel;
            }
        }

        let active_name = app.active_project.lock().await.clone();
        let mut proj_entries = Vec::new();
        let mut active_files = Vec::new();
        if let Ok(rd) = fs::read_dir(app.root.join("projects")) {
            for ent in rd.flatten() {
                let p = ent.path();
                if !p.is_dir() {
                    continue;
                }
                let Some(name) = p.file_name().and_then(|n| n.to_str()).map(ToString::to_string) else {
                    continue;
                };
                if name.starts_with('.') {
                    continue;
                }
                let cfg = WorkspaceConfig::load_or_default(&p);
                let prof = if name == "chat" { "chat".into() } else { cfg.profile_name };
                let has_git = p.join(".git").exists();
                let dirty = has_git && git_status_brief(&p).1;
                let mut files = Vec::new();
                list_workspace_files(&p, &p, false, &mut files);
                let file_count = files.len();
                if name == active_name {
                    active_files = files;
                }
                proj_entries.push(json!({
                    "name": name,
                    "profile_name": prof,
                    "has_git": has_git,
                    "dirty": dirty,
                    "file_count": file_count
                }));
            }
        }
        proj_entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));

        let sess_val = {
            let mut lock = app.sessions.lock().await;
            let sess = lock.entry(active_name.clone()).or_insert_with(|| app.load_or_init_session(&active_name));
            set_profile_max_tokens(sess.ctx_override);
            let (git, _) = git_status_brief(&sess.workspace);
            let task_md = tools::read_task_spec(&sess.workspace);
            let memory = fs::read_to_string(sess.workspace.join(".rs/memory.md")).unwrap_or_default();
            let r_str = match &sess.reasoning {
                Some(ReasoningSetting::Tokens(n)) => n.to_string(),
                Some(ReasoningSetting::Tier(t)) => t.clone(),
                None => String::new(),
            };
            json!({
                "name": sess.project_name,
                "path": sess.workspace.display().to_string(),
                "profile_name": sess.profile_name,
                "is_agent": sess.is_agent,
                "hud_summary": sess.hud_summary,
                "git": git,
                "task_md": task_md,
                "memory": memory,
                "files": active_files,
                "reasoning": r_str,
                "active_skills": sess.active_skills,
                "history": sess.ui_history,
                "last_stats": sess.last_stats
            })
        };

        let provider = ProviderConfig::load().await;
        let (slots, env_tok, _env_rnd) = read_env_slots(&get_env_path());
        let mcp_map = mcp::load_servers_config();
        let mut mcp_list: Vec<Value> = mcp_map
            .into_iter()
            .map(|(name, cfg)| json!({ "name": name, "command": cfg.command, "args": cfg.args }))
            .collect();
        mcp_list.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let max_rounds = match get_max_rounds() { usize::MAX => 0, r => r };
        let rail_cfg = RailConfig::load(&app.root.join("plugins/rs-rail/config.json"));

        return write_http_json(
            &mut stream,
            200,
            &json!({
                "projects": proj_entries,
                "active_project": sess_val,
                "profiles": Profile::list_available(),
                "skills": Skill::list_available(),
                "providers": slots,
                "provider": {
                    "model": provider.model,
                    "endpoint": format_endpoint(&provider.base_url),
                    "is_local": is_local_endpoint(&provider.base_url),
                    "max_tokens": get_max_tokens(),
                    "env_max_tokens": env_tok,
                    "max_rounds": max_rounds
                },
                "themes": load_themes(&app.root.join("plugins/rs-rail/themes")),
                "mcp_servers": mcp_list,
                "ui_prefs": {
                    "pad": state::get_pad(),
                    "show_thinking": state::get_show_thinking_summaries(),
                    "show_right_panel": rail_cfg.show_right_panel,
                    "hidden_projects": rail_cfg.hidden_projects,
                    "project_labels": rail_cfg.project_labels,
                    "project_names": rail_cfg.project_names
                }
            }),
        )
        .await;
    }

    if method == "POST" && !body.is_empty() && !is_json_ct {
        return write_http_err(&mut stream, 415, "Content-Type must be application/json").await;
    }
    let payload: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));


    if method == "POST" && path == "/api/noctalia/install" {
        let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let mode = payload.get("mode").and_then(|v| v.as_str()).unwrap_or("dark");
        return match install_noctalia_theme(&app, name, mode).await {
            Ok(res) => write_http_json(&mut stream, 200, &res).await,
            Err(e) => write_http_err(&mut stream, 400, &e.to_string()).await,
        };
    }

    if method == "POST" && path == "/api/themes/delete" {
        let id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("");
        return match delete_custom_theme(&app.root.join("plugins/rs-rail/themes"), id) {
            Ok(()) => write_http_json(&mut stream, 200, &json!({ "ok": true })).await,
            Err(e) => write_http_err(&mut stream, 400, &e.to_string()).await,
        };
    }

    if method == "POST" && path == "/api/authorize" {
        let req_id = payload.get("req_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let allow = payload.get("allow").and_then(|v| v.as_bool()).unwrap_or(false);
        if let Some(tx) = app.pending_perms.lock().await.remove(&req_id) {
            let _ = tx.send(allow);
        }
        return write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
    }

    if method == "POST" && path == "/api/stop" {
        let active = app.active_project.lock().await.clone();
        if let Some(sess) = app.sessions.lock().await.get(&active) {
            sess.cancel_flag.store(true, Ordering::SeqCst);
        }
        return write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
    }

    if method == "POST" && path == "/api/shutdown" {
        let _ = write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            std::process::exit(0);
        });
        return Ok(());
    }

    if method == "POST" && path == "/api/reset" {
        let active = app.active_project.lock().await.clone();
        state::clear_workspace_session(&app.root.join("projects").join(&active));
        let mut lock = app.sessions.lock().await;
        let next_gen = lock
            .get(&active)
            .map(|s| {
                s.cancel_flag.store(true, Ordering::SeqCst);
                s.generation + 1
            })
            .unwrap_or(1);
        let mut fresh = app.load_or_init_session(&active);
        fresh.generation = next_gen;
        lock.insert(active, fresh);
        return write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
    }

    if method == "POST" && path == "/api/export" {
        let active = app.active_project.lock().await.clone();
        let lock = app.sessions.lock().await;
        let Some(sess) = lock.get(&active) else {
            return write_http_err(&mut stream, 404, "Session not found").await;
        };
        let rel = export_session_markdown(sess)?;
        return write_http_json(&mut stream, 200, &json!({ "ok": true, "path": rel })).await;
    }

    if method == "POST" && path == "/api/memory" {
        let content = payload.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let active = app.active_project.lock().await.clone();
        let mut lock = app.sessions.lock().await;
        let sess = lock.entry(active.clone()).or_insert_with(|| app.load_or_init_session(&active));
        atomic_write(&sess.workspace.join(".rs/memory.md"), content)?;
        if let Some(loaded) = Profile::load(&sess.profile_name) {
            let sys = build_system_prompt(&loaded, &sess.workspace, sess.is_agent, &sess.active_skills);
            if let Some(first) = sess.llm_history.first_mut() {
                first.content = Value::String(sys);
            }
        }
        return write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
    }

    if method == "POST" && path == "/api/project/create" {
        let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_ascii_lowercase();
        let prof_name = payload.get("profile").and_then(|v| v.as_str()).unwrap_or("lfm3").trim();
        if !is_valid_slug(&name) || name == "chat" {
            return write_http_err(&mut stream, 400, "Invalid project slug").await;
        }
        let proj_dir = app.root.join("projects").join(&name);
        if proj_dir.join(".rs/config.json").exists() {
            return write_http_err(&mut stream, 409, "Project already exists").await;
        }
        let prof = Profile::load(prof_name).unwrap_or_else(|| Profile::parse("ROLE: Assistant."));
        let cfg = WorkspaceConfig {
            profile_name: prof_name.to_string(),
            is_agent: true,
            tier: prof.tier,
            adapters: prof.adapters,
            delegate: prof.delegate,
        };
        cfg.save(&proj_dir)?;
        set_last_project(&app.root, &name);
        app.sessions.lock().await.remove(&name);
        *app.active_project.lock().await = name.clone();
        return write_http_json(&mut stream, 200, &json!({ "name": name })).await;
    }

    if method == "POST" && path == "/api/project/clean" {
        let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
        if !is_valid_slug(name) {
            return write_http_err(&mut stream, 400, "Invalid project slug").await;
        }
        let projects_root = app.root.join("projects").canonicalize()?;
        let proj_dir = projects_root.join(name);
        if !proj_dir.is_dir() || !proj_dir.canonicalize().is_ok_and(|c| c.starts_with(&projects_root)) {
            return write_http_err(&mut stream, 404, "Project not found").await;
        }
        let removed = clean_project_dir(&proj_dir)?;
        return write_http_json(&mut stream, 200, &json!({ "ok": true, "removed": removed })).await;
    }

    if method == "POST" && path == "/api/file/delete" {
        let rel = payload.get("path").and_then(|v| v.as_str()).unwrap_or("").trim();
        let active = app.active_project.lock().await.clone();
        let proj_dir = app.root.join("projects").join(&active);
        let bnd = SecurityBoundary::new_headless(&proj_dir);
        let target = bnd.resolve_path(rel);
        if bnd.is_outside(&target) || is_protected_delete_path(rel, &target, &bnd.root) {
            return write_http_err(&mut stream, 403, "Forbidden path").await;
        }
        let Ok(meta) = fs::symlink_metadata(&target) else {
            return write_http_err(&mut stream, 404, "File not found").await;
        };
        if meta.file_type().is_symlink() || !meta.is_file() {
            return write_http_err(&mut stream, 400, "Not a regular file").await;
        }
        fs::remove_file(&target)?;
        if rel == ".rs/session.json" {
            let mut lock = app.sessions.lock().await;
            let next_gen = lock
                .get(&active)
                .map(|s| {
                    s.cancel_flag.store(true, Ordering::SeqCst);
                    s.generation + 1
                })
                .unwrap_or(1);
            let mut fresh = app.load_or_init_session(&active);
            fresh.generation = next_gen;
            lock.insert(active, fresh);
        }
        if let Some(parent) = target.parent() {
            if parent.file_name().is_some_and(|n| n == "__pycache__")
                && fs::read_dir(parent).is_ok_and(|mut r| r.next().is_none())
            {
                let _ = fs::remove_dir(parent);
            }
        }
        return write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
    }

    if method == "POST" && path == "/api/settings/provider" {
        let toggle = payload.get("toggle_slot").and_then(|v| v.as_str());
        let max_tok = payload.get("max_tokens").and_then(|v| v.as_u64()).map(|n| n as usize);
        let max_rnd = payload.get("max_rounds").and_then(|v| v.as_u64()).map(|n| n as usize);
        update_env_file(&get_env_path(), toggle, max_tok, max_rnd)?;
        return write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
    }

    if method == "POST" && path == "/api/settings/profile" {
        let active = app.active_project.lock().await.clone();
        let mut lock = app.sessions.lock().await;
        let sess = lock.entry(active.clone()).or_insert_with(|| app.load_or_init_session(&active));

        if let Some(new_prof) = payload.get("profile_name").and_then(|v| v.as_str()) {
            if sess.is_agent {
                if let Some(loaded) = Profile::load(new_prof) {
                    sess.profile_name = new_prof.to_string();
                    sess.tier = loaded.tier.clone();
                    sess.adapters = loaded.adapters;
                    sess.delegate = loaded.delegate;
                    sess.mcp_servers = loaded.mcp_servers.clone();
                    sess.mcp_tools = loaded.mcp_tools.clone();
                    sess.ctx_override = loaded.ctx;
                    sess.reasoning = loaded.reasoning_budget.clone();
                    sess.hud_summary = loaded.hud_summary(true);
                    set_profile_max_tokens(loaded.ctx);
                    let cfg = WorkspaceConfig {
                        profile_name: sess.profile_name.clone(),
                        is_agent: true,
                        tier: sess.tier.clone(),
                        adapters: sess.adapters,
                        delegate: sess.delegate,
                    };
                    let _ = cfg.save(&sess.workspace);
                    let sys = build_system_prompt(&loaded, &sess.workspace, true, &sess.active_skills);
                    if let Some(first) = sess.llm_history.first_mut() {
                        first.content = Value::String(sys);
                    }
                }
            }
        }

        if let Some(sk) = payload.get("skill_toggle").and_then(|v| v.as_str()) {
            if let Some(pos) = sess.active_skills.iter().position(|x| x == sk) {
                sess.active_skills.remove(pos);
            } else if sess.active_skills.len() < 3 && Skill::load(sk).is_some() {
                sess.active_skills.push(sk.to_string());
            }
            let loaded = Profile::load(&sess.profile_name).unwrap_or_else(|| Profile::parse("ROLE: Assistant."));
            let sys = build_system_prompt(&loaded, &sess.workspace, sess.is_agent, &sess.active_skills);
            if let Some(first) = sess.llm_history.first_mut() {
                first.content = Value::String(sys);
            }
        }

        if let Some(r_raw) = payload.get("reasoning").and_then(|v| v.as_str()) {
            sess.reasoning = if r_raw.is_empty() { None } else { ReasoningSetting::parse(r_raw) };
        }
        save_rail_session(sess);
        return write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
    }

    if method == "POST" && path == "/api/settings/ui" {
        if let Some(show) = payload.get("show_thinking").and_then(|v| v.as_bool()) {
            state::set_show_thinking_summaries(Some(show));
        }
        let cfg_path = app.root.join("plugins/rs-rail/config.json");
        let mut cfg = RailConfig::load(&cfg_path);
        let mut dirty_cfg = false;
        if let Some(show_rp) = payload.get("show_right_panel").and_then(|v| v.as_bool()) {
            cfg.show_right_panel = show_rp;
            dirty_cfg = true;
        }
        if let Some(arr) = payload.get("hidden_projects").and_then(|v| v.as_array()) {
            cfg.hidden_projects = sanitize_hidden_projects(arr);
            dirty_cfg = true;
        }
        if let Some(obj) = payload.get("project_labels").and_then(|v| v.as_object()) {
            cfg.project_labels = sanitize_project_labels(obj);
            dirty_cfg = true;
        }
        if let Some(obj) = payload.get("project_names").and_then(|v| v.as_object()) {
            cfg.project_names = sanitize_project_names(obj);
            dirty_cfg = true;
        }
        if payload.get("auto_label").and_then(|v| v.as_bool()) == Some(true) {
            apply_auto_labels(&app.root, &mut cfg);
            dirty_cfg = true;
        }
        if dirty_cfg {
            cfg.save(&cfg_path)?;
        }
        return write_http_json(
            &mut stream,
            200,
            &json!({
                "ok": true,
                "hidden_projects": cfg.hidden_projects,
                "project_labels": cfg.project_labels,
                "project_names": cfg.project_names
            }),
        )
        .await;
    }

    if method == "POST" && path == "/api/settings/mcp" {
        if let Some(srv) = payload.get("inspect").and_then(|v| v.as_str()) {
            match mcp::get_server_tools_schema(srv).await {
                Ok(tools) => return write_http_json(&mut stream, 200, &json!({ "tools": tools })).await,
                Err(e) => return write_http_err(&mut stream, 500, &e).await,
            }
        }
        if let Some(add_obj) = payload.get("add") {
            let name = add_obj.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
            let command = add_obj.get("command").and_then(|v| v.as_str()).unwrap_or("").trim();
            let args: Vec<String> = add_obj
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(ToString::to_string)).collect())
                .unwrap_or_default();
            if name.is_empty() || command.is_empty() {
                return write_http_err(&mut stream, 400, "Missing server name or command").await;
            }
            let mcp_path = app.root.join("plugins/mcp/servers.json");
            let mut servers = mcp::load_servers_config();
            servers.insert(name.to_string(), McpServerConfig { command: command.into(), args, env: HashMap::new() });
            atomic_write(&mcp_path, &serde_json::to_string_pretty(&json!({ "servers": servers }))?)?;
            return write_http_json(&mut stream, 200, &json!({ "ok": true })).await;
        }
        return write_http_err(&mut stream, 400, "Unknown MCP request").await;
    }

    if method == "POST" && path == "/api/chat" {
        let raw_msg = payload.get("message").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let images: Vec<String> = payload
            .get("images")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .filter(|s| s.starts_with("data:image/"))
                    .take(4)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if raw_msg.is_empty() && images.is_empty() {
            return write_http_err(&mut stream, 400, "Empty message").await;
        }

        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
        stream.write_all(head.as_bytes()).await?;

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let emit = move |ev: &str, data: Value| -> String { format!("event: {ev}\ndata: {data}\n\n") };
        let active = app.active_project.lock().await.clone();
        let req_cancel = {
            let mut lock = app.sessions.lock().await;
            let sess = lock.entry(active.clone()).or_insert_with(|| app.load_or_init_session(&active));
            sess.cancel_flag.store(false, Ordering::SeqCst);
            sess.cancel_flag.clone()
        };

        let app_clone = Arc::clone(&app);
        tokio::spawn(async move {
            let (
                workspace,
                is_agent,
                adapters,
                delegate,
                tier,
                mcp_servers,
                mcp_tools,
                ctx_override,
                reasoning,
                mut history,
                cancel_flag,
                start_gen,
            ) = {
                let mut lock = app_clone.sessions.lock().await;
                let sess = lock.entry(active.clone()).or_insert_with(|| app_clone.load_or_init_session(&active));
                (
                    sess.workspace.clone(),
                    sess.is_agent,
                    sess.adapters,
                    sess.delegate,
                    sess.tier.clone(),
                    sess.mcp_servers.clone(),
                    sess.mcp_tools.clone(),
                    sess.ctx_override,
                    sess.reasoning.clone(),
                    sess.llm_history.clone(),
                    sess.cancel_flag.clone(),
                    sess.generation,
                )
            };

            set_profile_max_tokens(ctx_override);
            let is_task = raw_msg == "/task" || raw_msg.starts_with("/task ");
            let effective_prompt = if is_task {
                let arg = raw_msg.strip_prefix("/task").unwrap_or("").trim();
                if !arg.is_empty() {
                    arg.to_string()
                } else if let Some(spec) = tools::read_task_spec(&workspace) {
                    spec
                } else {
                    let _ = tx.send(emit("error", json!({ "message": "No goal given and no TASK.md found." })));
                    let _ = tx.send(emit("done", json!({})));
                    return;
                }
            } else {
                raw_msg.clone()
            };

            let provider = ProviderConfig::load().await;
            if !images.is_empty() && is_local_endpoint(&provider.base_url) {
                let _ = tx.send(emit("error", json!({ "message": "Switch to a cloud model to send images." })));
                let _ = tx.send(emit("done", json!({})));
                return;
            }

            {
                let mut lock = app_clone.sessions.lock().await;
                if let Some(sess) = lock.get_mut(&active) {
                    if sess.generation == start_gen {
                        sess.ui_history.push(UiTurnItem {
                            role: "user".into(),
                            images: if images.is_empty() { None } else { Some(images.clone()) },
                            content: if raw_msg.is_empty() { None } else { Some(raw_msg) },
                            ..Default::default()
                        });
                    }
                }
            }

            history.push(build_user_chat_message(&effective_prompt, &images));
            let client = LlmClient::new(provider);
            let boundary = SecurityBoundary::new_headless(&workspace);
            let active_tools = if is_agent {
                Some(tools::get_tools_schema_with_mcp(&tier, delegate, &mcp_servers, &mcp_tools).await)
            } else {
                None
            };
            let max_rounds = if is_agent { get_max_rounds() } else { 1 };

            let mut new_ui_items = Vec::new();
            let mut last_stats = None;
            let mut round0_failed = false;

            for _round in 0..max_rounds {
                if cancel_flag.load(Ordering::SeqCst) {
                    if new_ui_items.is_empty() {
                        round0_failed = true;
                    }
                    break;
                }
                let tx_chunk = tx.clone();
                let llm_t0 = Instant::now();
                let think_ms = Arc::new(AtomicU64::new(0));
                let think_ms_c = Arc::clone(&think_ms);
                let res = client
                    .stream_chat_raw_with_reasoning(
                        &history,
                        active_tools.clone(),
                        Some(cancel_flag.clone()),
                        Some(reasoning.clone()),
                        move |slice, is_t| {
                            if is_t {
                                think_ms_c.store(llm_t0.elapsed().as_millis() as u64, Ordering::Relaxed);
                            }
                            let ev = if is_t { "thinking" } else { "chunk" };
                            let _ = tx_chunk.send(format!("event: {ev}\ndata: {}\n\n", json!({ "text": slice })));
                        },
                    )
                    .await;
                let llm_elapsed = llm_t0.elapsed().as_secs_f64().max(0.01);

                let (reply, opt_reasoning, tool_calls, stats) = match res {
                    Ok(v) => v,
                    Err(e) => {
                        let rolled_back = new_ui_items.is_empty();
                        if rolled_back {
                            round0_failed = true;
                        }
                        if !cancel_flag.load(Ordering::SeqCst) {
                            let _ = tx.send(emit("error", json!({ "message": e.to_string(), "rolled_back": rolled_back })));
                        }
                        break;
                    }
                };
                let prompt_tokens = if stats.prompt_tokens > 0 {
                    stats.prompt_tokens
                } else {
                    history.iter().map(|m| m.content_as_str().chars().count()).sum::<usize>() / 4
                };
                let completion_tokens = if stats.completion_tokens > 0 {
                    stats.completion_tokens
                } else {
                    (reply.chars().count() / 4).max(usize::from(!reply.is_empty()))
                };
                let reasoning_tokens = if stats.reasoning_tokens > 0 {
                    stats.reasoning_tokens
                } else {
                    opt_reasoning.as_ref().map(|r| (r.chars().count() / 4).max(1)).unwrap_or(0)
                };
                let gen_toks = completion_tokens + reasoning_tokens;
                let tps = if llm_elapsed > 0.01 { gen_toks as f64 / llm_elapsed } else { 0.0 };
                last_stats = Some(TurnStats {
                    prompt_tokens,
                    completion_tokens,
                    reasoning_tokens,
                    cached_tokens: stats.cached_tokens,
                    elapsed_s: llm_elapsed,
                    tps,
                });

                if !reply.is_empty() || opt_reasoning.is_some() {
                    let ms = think_ms.load(Ordering::Relaxed);
                    let think_s = opt_reasoning.as_ref().map(|_| if ms > 0 { (ms as f64 / 1000.0).max(0.1) } else { llm_elapsed });
                    new_ui_items.push(UiTurnItem {
                        role: "assistant".into(),
                        content: if reply.is_empty() { None } else { Some(reply.clone()) },
                        reasoning: opt_reasoning.clone(),
                        elapsed_s: think_s,
                        ..Default::default()
                    });
                }

                let Some(calls) = tool_calls.filter(|c| !c.is_empty() && is_agent) else {
                    if cancel_flag.load(Ordering::SeqCst) && reply.is_empty() && opt_reasoning.is_none() {
                        if new_ui_items.is_empty() {
                            round0_failed = true;
                        }
                        break;
                    }
                    history.push(ChatMessage::assistant_with_reasoning(reply.clone(), opt_reasoning));
                    if cancel_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    if is_task && !tools::is_task_complete(&reply) {
                        history.push(ChatMessage::user("Continue with the next action."));
                        continue;
                    }
                    break;
                };

                let mut assistant_msg = ChatMessage::assistant_with_reasoning(reply, opt_reasoning);
                assistant_msg.tool_calls = Some(Value::Array(calls.clone()));
                history.push(assistant_msg);

                let mut idx = 0;
                while idx < calls.len() {
                    if cancel_flag.load(Ordering::SeqCst) {
                        for rem in &calls[idx..] {
                            history.push(ChatMessage::tool_response(
                                rem["id"].as_str().unwrap_or("call_0").into(),
                                "[Cancelled]".into(),
                            ));
                        }
                        break;
                    }

                    let fn_name = calls[idx]["function"]["name"].as_str().unwrap_or("tool");
                    if tools::is_read_only_tool(fn_name) {
                        let mut batch_end = idx + 1;
                        while batch_end < calls.len()
                            && tools::is_read_only_tool(calls[batch_end]["function"]["name"].as_str().unwrap_or("tool"))
                        {
                            batch_end += 1;
                        }
                        let mut futs = Vec::new();
                        for c in &calls[idx..batch_end] {
                            let c_id = c["id"].as_str().unwrap_or("call_0").to_string();
                            let c_name = c["function"]["name"].as_str().unwrap_or("tool").to_string();
                            let raw_a = c["function"]["arguments"].as_str().unwrap_or("{}");
                            let args = if adapters { adapter::repair_tool_args(&c_name, raw_a) } else { raw_a.into() };
                            let brief = serde_json::from_str::<Value>(&args)
                                .ok()
                                .and_then(|v| {
                                    v.get("path")
                                        .or_else(|| v.get("pattern"))
                                        .and_then(|x| x.as_str())
                                        .map(ToString::to_string)
                                })
                                .unwrap_or_default();
                            let ws = workspace.clone();
                            let bnd = SecurityBoundary::new_headless(&ws);
                            futs.push(async move {
                                let t0 = Instant::now();
                                let out = tools::execute_tool_with_auth(&c_name, &args, &ws, &bnd, false).await;
                                (c_id, c_name, brief, out, t0.elapsed().as_secs_f64())
                            });
                        }
                        for (c_id, c_name, brief, res, elapsed) in futures_util::future::join_all(futs).await {
                            let (ok, summary, raw_txt, resp_txt) = match res {
                                Ok(o) => (true, format!("{} line(s)", o.lines().count()), Some(o.clone()), o),
                                Err(e) => (false, e.clone(), None, format!("[tool error] {e}")),
                            };
                            let item = UiTurnItem {
                                role: "tool_card".into(),
                                name: Some(c_name),
                                brief: Some(brief),
                                ok: Some(ok),
                                summary: Some(summary),
                                elapsed_s: Some(elapsed),
                                raw: raw_txt,
                                ..Default::default()
                            };
                            let _ = tx.send(emit("tool_result", serde_json::to_value(&item).unwrap_or(json!({}))));
                            new_ui_items.push(item);
                            history.push(ChatMessage::tool_response(c_id, resp_txt));
                        }
                        idx = batch_end;
                        continue;
                    }

                    let call = &calls[idx];
                    let call_id = call["id"].as_str().unwrap_or("call_0").to_string();
                    let raw_args = call["function"]["arguments"].as_str().unwrap_or("{}");
                    let final_args = if adapters { adapter::repair_tool_args(fn_name, raw_args) } else { raw_args.into() };
                    let parsed_args: Value = serde_json::from_str(&final_args).unwrap_or_else(|_| json!({}));
                    let brief = parsed_args
                        .get("path")
                        .or_else(|| parsed_args.get("command"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let mut preauthorized = false;
                    if fn_name == "run_command" {
                        if let Some(cmd_str) = parsed_args.get("command").and_then(|v| v.as_str()) {
                            if let Some(reason) = boundary.check_command(cmd_str) {
                                let req_id = app_clone.next_perm_id.fetch_add(1, Ordering::SeqCst);
                                let (ptx, prx) = oneshot::channel();
                                app_clone.pending_perms.lock().await.insert(req_id, ptx);
                                let _ = tx.send(emit(
                                    "permission",
                                    json!({ "req_id": req_id, "tool": fn_name, "title": reason, "detail": cmd_str }),
                                ));
                                let approved = tokio::select! {
                                    _ = async {
                                        while !cancel_flag.load(Ordering::Relaxed) {
                                            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                                        }
                                    } => false,
                                    r = prx => r.unwrap_or(false),
                                };
                                app_clone.pending_perms.lock().await.remove(&req_id);
                                if !approved {
                                    let msg = format!("Denied by user ({reason})");
                                    let item = UiTurnItem {
                                        role: "tool_card".into(),
                                        name: Some(fn_name.into()),
                                        brief: Some(brief),
                                        ok: Some(false),
                                        summary: Some(msg.clone()),
                                        elapsed_s: Some(0.0),
                                        ..Default::default()
                                    };
                                    let _ = tx.send(emit("tool_result", serde_json::to_value(&item).unwrap_or(json!({}))));
                                    new_ui_items.push(item);
                                    history.push(ChatMessage::tool_response(call_id, format!("[denied] {msg}")));
                                    idx += 1;
                                    continue;
                                }
                                preauthorized = true;
                            }
                        }
                    }

                    let pre_txt = if (fn_name == "edit_file" || fn_name == "write_file") && !brief.is_empty() {
                        let p = boundary.resolve_path(&brief);
                        if !boundary.is_outside(&p) { fs::read_to_string(&p).ok() } else { None }
                    } else {
                        None
                    };

                    let t0 = Instant::now();
                    let res = tools::execute_tool_with_auth(fn_name, &final_args, &workspace, &boundary, preauthorized).await;
                    let elapsed = t0.elapsed().as_secs_f64();

                    let (ok, summary, diff, raw_txt, resp_txt) = match res {
                        Ok(o) => {
                            if fn_name == "edit_file" || fn_name == "write_file" {
                                let p = boundary.resolve_path(&brief);
                                let new_txt = if !boundary.is_outside(&p) { fs::read_to_string(&p).ok() } else { None };
                                if let Some(nt) = new_txt {
                                    let d = compute_diff(pre_txt.as_deref().unwrap_or(""), &nt);
                                    (true, format!("Updated {} (+{} -{})", brief, d.adds, d.rems), Some(d), None, o)
                                } else {
                                    (true, format!("Updated {brief}"), None, None, o)
                                }
                            } else {
                                let first = o.lines().next().unwrap_or("(no output)").trim().to_string();
                                (true, first, None, Some(o.clone()), o)
                            }
                        }
                        Err(e) => (false, e.clone(), None, None, format!("[tool error] {e}")),
                    };

                    let item = UiTurnItem {
                        role: "tool_card".into(),
                        name: Some(fn_name.into()),
                        brief: Some(brief),
                        ok: Some(ok),
                        summary: Some(summary),
                        elapsed_s: Some(elapsed),
                        diff,
                        raw: raw_txt,
                        ..Default::default()
                    };
                    let _ = tx.send(emit("tool_result", serde_json::to_value(&item).unwrap_or(json!({}))));
                    new_ui_items.push(item);
                    history.push(ChatMessage::tool_response(call_id, resp_txt));
                    idx += 1;
                }
            }

            {
                let mut lock = app_clone.sessions.lock().await;
                if let Some(sess) = lock.get_mut(&active) {
                    if sess.generation == start_gen {
                        if round0_failed {
                            if sess.ui_history.last().is_some_and(|m| m.role == "user") {
                                sess.ui_history.pop();
                            }
                        } else {
                            sess.llm_history = history;
                            sess.ui_history.extend(new_ui_items);
                            if last_stats.is_some() {
                                sess.last_stats = last_stats;
                            }
                            save_rail_session(sess);
                        }
                    }
                }
            }
            let _ = tx.send(emit("done", json!({})));
        });

        while let Some(chunk) = rx.recv().await {
            if stream.write_all(chunk.as_bytes()).await.is_err() {
                req_cancel.store(true, Ordering::SeqCst);
                break;
            }
            let _ = stream.flush().await;
        }
        let _ = stream.shutdown().await;
        return Ok(());
    }

    write_http_err(&mut stream, 404, "Not found").await
}

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("RS_RAIL_PORT").unwrap_or_else(|_| "24243".into());
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!("● rs-rail listening on http://{addr}");

    let app = Arc::new(AppRuntime::new());
    loop {
        let (socket, _) = match listener.accept().await {
            Ok(p) => p,
            Err(_) => continue,
        };
        let app_ref = Arc::clone(&app);
        tokio::spawn(async move {
            let _ = handle_client(socket, app_ref).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_or_init_session_recreates_missing_config() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_init_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("projects/demo")).unwrap();

        let app = AppRuntime::new_in(tmp.clone());
        let sess = app.load_or_init_session("demo");
        assert!(sess.is_agent);
        assert!(tmp.join("projects/demo/.rs/config.json").is_file());

        fs::create_dir_all(tmp.join("projects/missingprof")).unwrap();
        WorkspaceConfig {
            profile_name: "no-such-profile-xyz".into(),
            is_agent: true,
            tier: ToolTier::Smol,
            adapters: false,
            delegate: false,
        }
        .save(tmp.join("projects/missingprof"))
        .unwrap();
        assert!(app.load_or_init_session("missingprof").profile_name.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_valid_slug_rejects_traversal() {
        assert!(is_valid_slug("nemo"));
        assert!(is_valid_slug("qwen38-c"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug(".."));
        assert!(!is_valid_slug("../etc"));
        assert!(!is_valid_slug("a/b"));
    }

    #[test]
    fn test_compute_diff_counts_and_lines() {
        let d = compute_diff("a\nb\nc\n", "a\nx\ny\nc\n");
        assert_eq!(d.adds, 2);
        assert_eq!(d.rems, 1);
    }

    #[test]
    fn test_env_toggle_preserves_keys_and_updates_governors() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_env_{}.env", std::process::id()));
        fs::write(
            &tmp,
            "#GEMINI_API_KEY=\"AIzaTest123\"\nOPENROUTER_API_KEY=\"sk-or-test\"\nAI_MAX_TOKENS=\"8192\"\nAI_MAX_AGENT_ROUNDS=\"10\"\n",
        )
        .unwrap();

        update_env_file(&tmp, Some("GEMINI"), Some(65536), Some(20)).unwrap();
        let (slots, tok, rnd) = read_env_slots(&tmp);
        let _ = fs::remove_file(&tmp);

        assert!(slots.iter().find(|s| s.id == "GEMINI").unwrap().is_active);
        assert!(!slots.iter().find(|s| s.id == "OPENROUTER").unwrap().is_active);
        assert_eq!(tok, 65536);
        assert_eq!(rnd, 20);
    }

    #[test]
    fn test_is_env_key_prevents_prefix_collision_and_filters_placeholders() {
        assert!(is_env_key("GEMINI_API_KEY=abc", "GEMINI_API_KEY"));
        assert!(!is_env_key("GEMINI_API_KEY_ALT=abc", "GEMINI_API_KEY"));
        assert!(!is_valid_slot_val("your-key-here"));
        assert!(is_valid_slot_val("sk-real-key"));
    }

    #[test]
    fn test_load_themes_parses_omarchy_noctalia_and_custom_folder() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_themes_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("themes")).unwrap();
        fs::write(tmp.join("themes/claude.json"), r##"{"name":"Claude","bg":"#141413","accent":"#d97757"}"##).unwrap();
        fs::write(tmp.join("colors.toml"), "background = \"#0a1220\"\naccent = \"#78dce8\" # hex comment\n").unwrap();

        let om = parse_omarchy_toml(&tmp).unwrap();
        assert_eq!(om["accent"], "#78dce8");
        assert_eq!(om["mode"], "dark");


        let custom = load_themes(&tmp.join("themes"));
        assert!(custom.iter().any(|t| t["id"] == "custom:claude" && t["accent"] == "#d97757"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_clean_project_dir_preserves_rs_and_removes_files() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_clean_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".rs")).unwrap();
        fs::create_dir_all(tmp.join("sub")).unwrap();
        fs::write(tmp.join(".rs/config.json"), "{}").unwrap();
        fs::write(tmp.join("test_calc.py"), "print(1)").unwrap();
        fs::write(tmp.join("sub/a.txt"), "hi").unwrap();

        let removed = clean_project_dir(&tmp).unwrap();
        assert_eq!(removed, 2);
        assert!(tmp.join(".rs/config.json").is_file());
        assert!(!tmp.join("test_calc.py").exists());
        assert!(!tmp.join("sub").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_build_user_chat_message_supports_multimodal_images() {
        let text_only = build_user_chat_message("hello", &[]);
        assert_eq!(text_only.content, Value::String("hello".into()));

        let mm = build_user_chat_message("look", &["data:image/png;base64,AAAA".into()]);
        let arr = mm.content.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[1]["image_url"]["url"], "data:image/png;base64,AAAA");
    }

    #[test]
    fn test_list_workspace_files_url_decode_and_export_markdown() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_ws_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".rs")).unwrap();
        fs::create_dir_all(tmp.join("src")).unwrap();
        fs::write(tmp.join(".rs/config.json"), "{}").unwrap();
        fs::write(tmp.join("a.py"), "x=1").unwrap();
        fs::write(tmp.join("src/b.rs"), "fn main(){}").unwrap();

        let mut files = Vec::new();
        fs::create_dir_all(tmp.join("__pycache__")).unwrap();
        fs::write(tmp.join("__pycache__/a.pyc"), "pyc").unwrap();
        list_workspace_files(&tmp, &tmp, false, &mut files);
        assert_eq!(files, vec!["__pycache__/a.pyc", "a.py", "src/b.rs"]);
        let mut with_rs = Vec::new();
        list_workspace_files(&tmp, &tmp, true, &mut with_rs);
        assert_eq!(with_rs, vec![".rs/config.json", "__pycache__/a.pyc", "a.py", "src/b.rs"]);

        let rcfg_path = tmp.join("rail_cfg.json");
        assert!(RailConfig::load(&rcfg_path).show_right_panel);
        RailConfig { show_right_panel: false, last_project: Some("ling-tiny".into()), ..Default::default() }.save(&rcfg_path).unwrap();
        let loaded_rcfg = RailConfig::load(&rcfg_path);
        assert!(!loaded_rcfg.show_right_panel);
        assert_eq!(loaded_rcfg.last_project.as_deref(), Some("ling-tiny"));
        assert_eq!(url_decode("src%2Fb%20c.rs"), "src/b c.rs");

        let sess = SessionState {
            project_name: "demo".into(),
            workspace: tmp.clone(),
            profile_name: "demo".into(),
            is_agent: true,
            adapters: false,
            delegate: false,
            tier: ToolTier::Core4,
            mcp_servers: Vec::new(),
            mcp_tools: Vec::new(),
            ctx_override: None,
            hud_summary: String::new(),
            reasoning: None,
            active_skills: Vec::new(),
            llm_history: Vec::new(),
            ui_history: vec![UiTurnItem { role: "user".into(), content: Some("hi".into()), ..Default::default() }],
            last_stats: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            generation: 0,
        };
        let rel = export_session_markdown(&sess).unwrap();
        assert!(rel.starts_with(".rs/transcripts/chat_"));
        assert!(tmp.join(&rel).is_file());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_local_host_and_origin_rejects_subdomain_spoofing() {
        assert!(is_local_host("127.0.0.1:24243"));
        assert!(is_local_host("localhost:24243"));
        assert!(is_local_host("[::1]:24243"));
        assert!(!is_local_host("localhost.evil.com:24243"));
        assert!(!is_local_host("127.0.0.1.evil.com"));

        assert!(is_local_origin("http://127.0.0.1:24243"));
        assert!(is_local_origin("http://localhost"));
        assert!(!is_local_origin("http://localhost.evil.com:24243"));
    }

    #[test]
    fn test_env_custom_slot_reads_commented_model_and_skips_placeholders() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_custom_{}.env", std::process::id()));
        fs::write(
            &tmp,
            "#GEMINI_API_KEY=\"AQ.your-key-here\"\n# CUSTOM2_NAME=\"AMD Token Lab\"\n# CUSTOM2_API_KEY=\"tl-real-123\"\n# CUSTOM2_URL=\"https://api.tokenlab.amd.com/v1/chat/completions\"\n# CUSTOM2_MODEL=\"DeepSeek-V4.1-Flash\"\n",
        )
        .unwrap();

        let (slots, _, _) = read_env_slots(&tmp);
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].id, "CUSTOM2");
        assert_eq!(slots[0].model, "DeepSeek-V4.1-Flash");

        update_env_file(&tmp, Some("CUSTOM2"), None, None).unwrap();
        let saved = fs::read_to_string(&tmp).unwrap();
        let _ = fs::remove_file(&tmp);

        assert!(saved.contains("CUSTOM2_API_KEY=\"tl-real-123\""));
        assert!(saved.contains("CUSTOM2_URL=\"https://api.tokenlab.amd.com/v1/chat/completions\""));
        assert!(saved.contains("#GEMINI_API_KEY=\"AQ.your-key-here\""));
    }

    #[test]
    fn test_should_shutdown_guards_reload_race() {
        assert!(should_shutdown(0, 3, 3));
        assert!(!should_shutdown(1, 4, 3));
        assert!(!should_shutdown(0, 4, 3));
    }

    #[test]
    fn test_ui_items_from_messages_reconstructs_cli_turns() {
        let mut assistant_tc = ChatMessage::assistant("");
        assistant_tc.tool_calls = Some(json!([{
            "id": "call_1",
            "type": "function",
            "function": { "name": "read_file", "arguments": "{\"path\":\"a.py\"}" }
        }]));
        let msgs = vec![
            ChatMessage::user("inspect a.py"),
            assistant_tc,
            ChatMessage::tool_response("call_1".into(), "print(1)\n".into()),
            ChatMessage::assistant_with_reasoning("Looks good.", Some("checked".into())),
        ];
        let items = ui_items_from_messages(&msgs);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].role, "user");
        assert_eq!(items[1].role, "tool_card");
        assert_eq!(items[1].brief.as_deref(), Some("a.py"));
        assert_eq!(items[2].role, "assistant");
        assert_eq!(items[2].reasoning.as_deref(), Some("checked"));
    }

    #[test]
    fn test_noctalia_live_parses_v4_colors_json_and_v5_settings_toml() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_noc_live_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let cfg = tmp.join("cfg");
        let st = tmp.join("state");
        fs::create_dir_all(st.join("community-palettes")).unwrap();
        fs::create_dir_all(&cfg).unwrap();

        // v5 settings.toml + cached community palette
        fs::write(
            st.join("settings.toml"),
            "[theme]\nsource = \"community\"\ncommunity_palette = \"Gruvbox Material\"\nmode = \"light\"\n",
        )
        .unwrap();
        fs::write(
            st.join("community-palettes/Gruvbox Material.json"),
            r##"{"light":{"surface":"#fbf1c7","surface_container":"#f2e5bc","outline_variant":"#bdae93","on_surface":"#3c3836","on_surface_variant":"#665c54","primary":"#6c782e","secondary":"#b47109"}}"##,
        )
        .unwrap();
        let t_v5 = parse_noctalia_live_in_with_themes(&cfg, &st, &cfg).unwrap();
        assert_eq!(t_v5["id"], "noctalia");
        assert_eq!(t_v5["source"], "live");
        assert_eq!(t_v5["mode"], "light");
        assert_eq!(t_v5["bg"], "#fbf1c7");

        // v4 / template colors.json takes precedence
        fs::write(
            cfg.join("colors.json"),
            r##"{"mSurface":"#212337","mSurfaceVariant":"#323449","mOutline":"#454759","mOnSurface":"#ebfafa","mOnSurfaceVariant":"#a4b1cd","mPrimary":"#37f499","mSecondary":"#04d1f9"}"##,
        )
        .unwrap();
        let t_v4 = parse_noctalia_live_in_with_themes(&cfg, &st, &cfg).unwrap();
        assert_eq!(t_v4["mode"], "dark");
        assert_eq!(t_v4["accent"], "#37f499");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_noctalia_live_wallpaper_uses_template_json() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_noc_wall_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let cfg = tmp.join("cfg");
        let st = tmp.join("state");
        let th = tmp.join("themes");
        fs::create_dir_all(&cfg).unwrap();
        fs::create_dir_all(&st).unwrap();
        fs::create_dir_all(&th).unwrap();
        fs::write(st.join("settings.toml"), "[theme]\nsource = \"wallpaper\"\nbuiltin = \"Nord\"\nmode = \"dark\"\n").unwrap();
        fs::write(th.join("noctalia.json"), r##"{"name":"Noctalia","mode":"dark","bg":"#292714","accent":"#e4d467","cyan":"#a9d65c"}"##).unwrap();
        let t = parse_noctalia_live_in_with_themes(&cfg, &st, &th).unwrap();
        assert_eq!(t["id"], "noctalia");
        assert_eq!(t["source"], "live");
        assert_eq!(t["bg"], "#292714");
        assert_eq!(t["accent"], "#e4d467");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_delete_custom_theme_enforces_containment() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_del_theme_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("claude.json"), "{}").unwrap();

        assert_eq!(palette_slug("Rosé Pine", "dark"), "rose-pine");
        assert_eq!(palette_slug("Catppuccin Mocha", "light"), "catppuccin-mocha-light");
        assert!(delete_custom_theme(&tmp, "omarchy").is_err());
        assert!(delete_custom_theme(&tmp, "noctalia").is_err());
        assert!(delete_custom_theme(&tmp, "custom:../claude").is_err());
        delete_custom_theme(&tmp, "custom:claude").unwrap();
        assert!(!tmp.join("claude.json").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_builtin_noctalia_list_is_core_and_offline() {
        let list = builtin_noctalia_list();
        assert_eq!(list.len(), BUILTIN_NOCTALIA.len());
        assert!(list.iter().all(|v| v["core"] == true && v["base"] == "Core"));
    }

    #[test]
    fn test_git_status_brief_ignores_parent_rs_agent_repo() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_git_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join("projects/nogit")).unwrap();
        let (lbl, dirty) = git_status_brief(&tmp.join("projects/nogit"));
        assert_eq!(lbl, "");
        assert!(!dirty);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_auto_labels_and_rail_config_persist_respects_manual_overrides() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_labels_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("profiles/artcraft")).unwrap();
        fs::create_dir_all(tmp.join("projects/photocraft")).unwrap();
        fs::create_dir_all(tmp.join("projects/nemo")).unwrap();
        fs::create_dir_all(tmp.join("projects/custom")).unwrap();
        fs::create_dir_all(tmp.join("projects/chat")).unwrap();

        fs::write(tmp.join("profiles/artcraft/photocraft.md"), "---\nmcp: photocraft\n---\nhi").unwrap();
        fs::write(tmp.join("profiles/nemo.md"), "---\ncore4: true\n---\nhi").unwrap();
        fs::write(tmp.join("profiles/custom.md"), "---\nlabels: [\"mcp-work\", \"rust\"]\n---\nhi").unwrap();

        WorkspaceConfig { profile_name: "artcraft/photocraft".into(), is_agent: true, tier: ToolTier::Smol, adapters: true, delegate: false }
            .save(tmp.join("projects/photocraft")).unwrap();
        WorkspaceConfig { profile_name: "nemo".into(), is_agent: true, tier: ToolTier::Core4, adapters: false, delegate: false }
            .save(tmp.join("projects/nemo")).unwrap();

        let mut cfg = RailConfig::default();
        cfg.project_labels.insert("nemo".into(), vec!["my-cloud".into()]); // manual override must stick
        cfg.project_labels.insert("photocraft".into(), vec!["local".into()]); // stale auto-tag refreshes on profile change
        let added = apply_auto_labels(&tmp, &mut cfg);
        assert_eq!(added, 2);
        assert_eq!(cfg.project_labels.get("photocraft").unwrap(), &vec!["mcp".to_string()]);
        assert_eq!(cfg.project_labels.get("custom").unwrap(), &vec!["mcp-work".to_string(), "rust".to_string()]);
        assert_eq!(cfg.project_labels.get("nemo").unwrap(), &vec!["my-cloud".to_string()]);
        assert!(!cfg.project_labels.contains_key("chat"));

        cfg.hidden_projects = vec!["custom".into()];
        let mut raw_names = serde_json::Map::new();
        raw_names.insert("nemo".into(), json!("Cloud API"));
        raw_names.insert("chat".into(), json!("chat")); // identical to slug -> omitted
        raw_names.insert("../bad".into(), json!("nope"));
        cfg.project_names = sanitize_project_names(&raw_names);
        assert_eq!(cfg.project_names.get("nemo").map(String::as_str), Some("Cloud API"));
        assert!(!cfg.project_names.contains_key("chat"));
        assert!(!cfg.project_names.contains_key("../bad"));

        let cfg_p = tmp.join("config.json");
        cfg.save(&cfg_p).unwrap();
        let reloaded = RailConfig::load(&cfg_p);
        assert_eq!(reloaded.hidden_projects, vec!["custom".to_string()]);
        assert_eq!(reloaded.project_labels.get("photocraft").unwrap(), &vec!["mcp".to_string()]);
        assert_eq!(reloaded.project_names.get("nemo").map(String::as_str), Some("Cloud API"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_protected_delete_path_and_empty_labels_cleanup() {
        let tmp = std::env::temp_dir().join(format!("rs_rail_prot_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".rs")).unwrap();
        fs::write(tmp.join(".rs/config.json"), "{}").unwrap();
        fs::write(tmp.join("notes.txt"), "hi").unwrap();

        assert!(is_protected_delete_path("", &tmp, &tmp));
        assert!(is_protected_delete_path(".rs/config.json", &tmp.join(".rs/config.json"), &tmp));
        assert!(is_protected_delete_path("./.rs/config.json", &tmp.join("./.rs/config.json"), &tmp));
        assert!(is_protected_delete_path(".rs//memory.md", &tmp.join(".rs//memory.md"), &tmp));
        assert!(is_protected_delete_path(".git/HEAD", &tmp.join(".git/HEAD"), &tmp));
        assert!(is_protected_delete_path(".env", &tmp.join(".env"), &tmp));
        assert!(!is_protected_delete_path(".rs/session.json", &tmp.join(".rs/session.json"), &tmp));
        assert!(!is_protected_delete_path("notes.txt", &tmp.join("notes.txt"), &tmp));

        let mut raw = serde_json::Map::new();
        raw.insert("demo".into(), json!([]));
        raw.insert("nemo".into(), json!(["cloud"]));
        let cleaned = sanitize_project_labels(&raw);
        assert!(!cleaned.contains_key("demo"));
        assert_eq!(cleaned.get("nemo").unwrap(), &vec!["cloud".to_string()]);
        let _ = fs::remove_dir_all(&tmp);
    }
}
