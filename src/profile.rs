use crate::config::ToolTier;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReasoningSetting {
    Tokens(usize),
    Tier(String),
}

impl ReasoningSetting {
    pub fn parse(raw: &str) -> Option<Self> {
        let clean = raw.trim().trim_matches('"').trim_matches('\'').trim();
        if let Ok(n) = clean.parse::<usize>() {
            return Some(Self::Tokens(n));
        }
        let lower = clean.to_ascii_lowercase();
        if matches!(lower.as_str(), "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max") {
            return Some(Self::Tier(lower));
        }
        None
    }

    pub fn to_local_tokens(&self) -> usize {
        match self {
            Self::Tokens(n) => *n,
            Self::Tier(t) => match t.as_str() {
                "none" | "minimal" => 0,
                "low" => 250,
                "medium" => 500,
                "high" => 1000,
                "xhigh" | "max" => 4000,
                _ => 500,
            },
        }
    }

    pub fn to_cloud_tier(&self, is_google: bool, model: &str) -> String {
        let raw = match self {
            Self::Tier(t) => t.as_str(),
            Self::Tokens(n) => {
                if *n == 0 {
                    "none"
                } else if *n <= 350 {
                    "low"
                } else if *n <= 750 {
                    "medium"
                } else if *n <= 2000 {
                    "high"
                } else {
                    "xhigh"
                }
            }
        };

        if is_google {
            let m = model.to_lowercase();
            let no_minimal =
                m.contains("gemini-4") || m.contains("gemini-3.8") || m.contains("gemini-3.7") || m.contains("-pro");
            let no_none = m.contains("gemini-3") || m.contains("gemini-4") || m.contains("pro");
            if no_minimal && matches!(raw, "none" | "minimal") {
                return "low".to_string();
            }
            if no_none && raw == "none" {
                return "minimal".to_string();
            }
            if matches!(raw, "xhigh" | "max") {
                return if m.contains("gemini-4-hydrogen") { "xhigh" } else { "high" }.to_string();
            }
        }
        raw.to_string()
    }
}

use crate::config::strip_inline_comment;

#[derive(Debug, Clone)]
pub struct Profile {
    pub reasoning_budget: Option<ReasoningSetting>,
    pub tier: ToolTier,
    pub adapters: bool,
    pub memory: bool,
    pub delegate: bool,
    pub system_prompt: String,
    pub mcp_servers: Vec<String>,
    pub mcp_tools: Vec<String>,
    pub ctx: Option<usize>,
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        print!("\x1b[?25h");
        let _ = io::stdout().flush();
    }
}

#[derive(Clone, Debug)]
enum MenuRow {
    Root { name: String },
    Category { name: String, count: usize, expanded: bool },
    Child { category: String, name: String, is_last: bool },
}

impl MenuRow {
    fn profile_id(&self) -> Option<String> {
        match self {
            MenuRow::Root { name } => Some(name.clone()),
            MenuRow::Child { category, name, .. } => Some(format!("{}/{}", category, name)),
            MenuRow::Category { .. } => None,
        }
    }
}

impl Profile {
    pub fn parse(raw_content: &str) -> Self {
        let mut reasoning_budget = None;
        let mut tier = ToolTier::Smol;
        let mut adapters = false;
        let mut memory = false;
        let mut delegate = false;
        let mut mcp_servers = Vec::new();
        let mut mcp_tools = Vec::new();
        let mut ctx = None;
        let mut body = String::new();
        let mut in_frontmatter = false;
        let mut frontmatter_closed = false;

        for line in raw_content.lines() {
            let trimmed = line.trim();
            if trimmed == "---" {
                if !in_frontmatter && !frontmatter_closed {
                    in_frontmatter = true;
                    continue;
                } else if in_frontmatter {
                    in_frontmatter = false;
                    frontmatter_closed = true;
                    continue;
                }
            }

            if in_frontmatter {
                if let Some((k, v)) = trimmed.split_once(':') {
                    let key = k.trim();
                    let val = strip_inline_comment(v.trim());
                    match key {
                        "reasoning_budget" => {
                            if let Some(rs) = ReasoningSetting::parse(val) {
                                reasoning_budget = Some(rs);
                            }
                        }
                        "tier" if val.eq_ignore_ascii_case("purebash") => {
                            tier = ToolTier::PureBash;
                        }
                        "tier" if val.eq_ignore_ascii_case("core4") => {
                            tier = ToolTier::Core4;
                        }
                        "purebash" if val.eq_ignore_ascii_case("true") => {
                            tier = ToolTier::PureBash;
                        }
                        "core4" if val.eq_ignore_ascii_case("true") => {
                            tier = ToolTier::Core4;
                        }
                        "adapters" => {
                            adapters = val.eq_ignore_ascii_case("true");
                        }
                        "memory" => {
                            memory = val.eq_ignore_ascii_case("true");
                        }
                        "delegate" | "subagent" | "subagents" => {
                            delegate = val.eq_ignore_ascii_case("true");
                        }
                        "mcp" => {
                            let clean = val.trim_matches('[').trim_matches(']').trim();
                            for item in clean.split(',') {
                                let s = item.trim().trim_matches('"').trim_matches('\'').trim();
                                if !s.is_empty() {
                                    mcp_servers.push(s.to_string());
                                }
                            }
                        }
                        "ctx" | "context_size" => {
                            let s = val.trim_matches(['"', '\'']).trim();
                            let n = s.parse::<usize>().ok()
                                .or_else(|| s.strip_suffix(['k', 'K']).and_then(|v| v.parse::<usize>().ok().map(|x| x.saturating_mul(1024))))
                                .or_else(|| s.strip_suffix(['m', 'M']).and_then(|v| v.parse::<usize>().ok().map(|x| x.saturating_mul(1048576))));
                            if let Some(x) = n.filter(|&x| x > 0) {
                                ctx = Some(x);
                            }
                        }
                        "mcp_tools" => {
                            let clean = val.trim_matches('[').trim_matches(']').trim();
                            for item in clean.split(',') {
                                let s = item.trim().trim_matches('"').trim_matches('\'').trim();
                                if !s.is_empty() {
                                    mcp_tools.push(s.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                body.push_str(line);
                body.push('\n');
            }
        }

        Self {
            reasoning_budget,
            tier,
            adapters,
            memory,
            delegate,
            system_prompt: body.trim().to_string(),
            mcp_servers,
            mcp_tools,
            ctx,
        }
    }

    pub fn hud_summary(&self, is_agent: bool) -> String {
        let mut parts: Vec<String> = Vec::new();
        if is_agent {
            match self.tier {
                ToolTier::PureBash => parts.push("purebash".to_string()),
                ToolTier::Core4 => parts.push("core4".to_string()),
                ToolTier::Smol => {}
            }
        } else {
            parts.push("no tools".to_string());
        }
        if let Some(ref rb) = self.reasoning_budget {
            let v = match rb {
                ReasoningSetting::Tokens(n) => n.to_string(),
                ReasoningSetting::Tier(t) => t.clone(),
            };
            parts.push(v);
        }
        if self.adapters {
            parts.push("adp".to_string());
        }
        if self.memory {
            parts.push("mem".to_string());
        }
        if self.delegate {
            parts.push("del".to_string());
        }
        if !self.mcp_servers.is_empty() {
            if self.mcp_tools.is_empty() {
                parts.push(format!("mcp:{}", self.mcp_servers.join(",")));
            } else {
                parts.push(format!("mcp:{}({})", self.mcp_servers.join(","), self.mcp_tools.len()));
            }
        }
        if !is_agent && parts.len() == 1 {
            parts.push("pure chat".to_string());
        }
        parts.join(" · ")
    }

    pub fn profile_dirs() -> Vec<PathBuf> {
        dirs::home_dir()
            .map(|h| vec![h.join(".config/rs-agent/profiles")])
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.is_dir())
            .collect()
    }

    pub fn load(profile_name: &str) -> Option<Self> {
        let norm_target = profile_name.replace(['-', '_'], "").to_lowercase();
        let target_stem = profile_name.split('/').next_back().unwrap_or(profile_name);
        let target_stem_norm = target_stem.replace(['-', '_'], "").to_lowercase();

        for dir in Self::profile_dirs() {
            let direct = dir.join(format!("{}.md", profile_name));
            if direct.is_file() {
                if let Ok(content) = fs::read_to_string(&direct) {
                    return Some(Self::parse(&content));
                }
            }

            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let dir_name = entry.file_name().to_string_lossy().to_string();
                        if dir_name.starts_with('.') {
                            continue;
                        }

                        let sub_direct = path.join(format!("{}.md", target_stem));
                        if sub_direct.is_file() {
                            if let Some((cat, _)) = profile_name.split_once('/') {
                                if cat.eq_ignore_ascii_case(&dir_name) {
                                    if let Ok(content) = fs::read_to_string(&sub_direct) {
                                        return Some(Self::parse(&content));
                                    }
                                }
                            } else if let Ok(content) = fs::read_to_string(&sub_direct) {
                                return Some(Self::parse(&content));
                            }
                        }

                        if let Ok(sub_entries) = fs::read_dir(&path) {
                            for sub_entry in sub_entries.flatten() {
                                let sub_path = sub_entry.path();
                                if sub_path.extension().and_then(|e| e.to_str()) == Some("md") {
                                    if let Some(stem) = sub_path.file_stem().and_then(|s| s.to_str()) {
                                        let full_id = format!("{}/{}", dir_name, stem);
                                        let full_norm = full_id.replace(['-', '_'], "").to_lowercase();
                                        let stem_norm = stem.replace(['-', '_'], "").to_lowercase();

                                        if full_norm == norm_target
                                            || stem_norm == norm_target
                                            || stem_norm == target_stem_norm
                                        {
                                            if let Ok(content) = fs::read_to_string(&sub_path) {
                                                return Some(Self::parse(&content));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if stem.replace(['-', '_'], "").to_lowercase() == norm_target {
                                if let Ok(content) = fs::read_to_string(&path) {
                                    return Some(Self::parse(&content));
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn list_available() -> Vec<String> {
        let mut seen = HashSet::new();
        let mut list = Vec::new();

        for dir in Self::profile_dirs() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let dir_name = entry.file_name().to_string_lossy().to_string();
                        if dir_name.starts_with('.') {
                            continue;
                        }
                        if let Ok(sub_entries) = fs::read_dir(&path) {
                            for sub_entry in sub_entries.flatten() {
                                let sub_path = sub_entry.path();
                                if sub_path.extension().and_then(|e| e.to_str()) == Some("md") {
                                    if let Some(stem) = sub_path.file_stem().and_then(|s| s.to_str()) {
                                        let id = format!("{}/{}", dir_name, stem);
                                        if stem != "chat" && seen.insert(id.clone()) {
                                            list.push(id);
                                        }
                                    }
                                }
                            }
                        }
                    } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if stem != "chat" && seen.insert(stem.to_string()) {
                                list.push(stem.to_string());
                            }
                        }
                    }
                }
            }
        }

        list.sort_by(|a, b| {
            let a_nested = a.contains('/');
            let b_nested = b.contains('/');
            match (a_nested, b_nested) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => a.cmp(b),
            }
        });

        list
    }

    fn is_local_env(base_url: Option<&str>, model_name: &str) -> bool {
        if base_url.map(crate::client::is_local_endpoint).unwrap_or(false) {
            return true;
        }
        let m = model_name.to_lowercase();
        m.contains("local") || m.contains("llama-server") || m.contains("lfm") || m.contains("gguf")
    }

    pub fn find_workspace_candidates(ws_name: &str, available: &[String]) -> Vec<String> {
        let ws_clean = ws_name.replace(['-', '_'], "").to_lowercase();
        let mut exact = Vec::new();
        let mut prefix = Vec::new();

        for opt in available {
            let bare = opt.split('/').next_back().unwrap_or(opt);
            let opt_clean = opt.replace(['-', '_'], "").to_lowercase();
            let bare_clean = bare.replace(['-', '_'], "").to_lowercase();

            if opt_clean == ws_clean || bare_clean == ws_clean {
                exact.push(opt.clone());
            } else if ws_clean.len() >= 2
                && (opt_clean.starts_with(&ws_clean) || bare_clean.starts_with(&ws_clean))
                && !prefix.contains(opt)
            {
                prefix.push(opt.clone());
            }
        }

        if !exact.is_empty() {
            exact
        } else {
            prefix
        }
    }

    pub fn find_model_candidates(model_name: &str, available: &[String]) -> Vec<String> {
        let m_lower = model_name.to_lowercase();
        let m_clean = m_lower.replace(['-', '_', '.', '/'], "");
        let mut matches = Vec::new();

        let tokens: Vec<&str> = model_name.split(|c: char| !c.is_alphanumeric()).filter(|s| s.len() >= 3).collect();

        for opt in available {
            let bare = opt.split('/').next_back().unwrap_or(opt);
            let opt_lower = opt.to_lowercase();
            let bare_lower = bare.to_lowercase();
            let opt_clean = opt_lower.replace(['-', '_'], "");
            let bare_clean = bare_lower.replace(['-', '_'], "");

            if (m_clean.contains(&bare_clean)
                || bare_clean.contains(&m_clean)
                || m_clean.contains(&opt_clean)
                || opt_clean.contains(&m_clean))
                && !matches.contains(opt)
            {
                matches.push(opt.clone());
                continue;
            }

            for token in &tokens {
                let tok_lower = token.to_lowercase();
                if (bare_lower.contains(&tok_lower) || tok_lower.contains(&bare_lower)) && !matches.contains(opt) {
                    matches.push(opt.clone());
                    break;
                }
            }

            if (tokens.iter().any(|t| t.starts_with("gem")) || m_lower.contains("gem"))
                && (opt_lower.contains("gem") || bare_lower.contains("gem"))
                && !matches.contains(opt)
            {
                matches.push(opt.clone());
            }
        }
        matches
    }

    pub fn resolve_for_workspace(ws_name: &str, model_name: &str, base_url: Option<&str>) -> String {
        let available = Self::list_available();
        if available.is_empty() {
            return ws_name.to_string();
        }

        let mut candidates = Self::find_workspace_candidates(ws_name, &available);

        if candidates.is_empty() {
            candidates = Self::find_model_candidates(model_name, &available);
        }

        if candidates.is_empty() {
            candidates = available.clone();
        }

        if candidates.len() == 1 {
            return candidates[0].clone();
        }

        let is_local = Self::is_local_env(base_url, model_name);
        let default_idx = candidates
            .iter()
            .position(|c| {
                let cl = c.to_lowercase();
                if is_local {
                    cl.contains("loc") || cl.contains("local")
                } else if model_name.to_lowercase().contains("gem") {
                    cl.contains("gem")
                } else if model_name.to_lowercase().contains("grok") {
                    cl.contains("grok")
                } else if model_name.to_lowercase().contains("claude") {
                    cl.contains("claude") || cl.contains("sonnet")
                } else {
                    false
                }
            })
            .unwrap_or(0);

        let title = format!("Multiple profiles match \x1b[1;33m{}\x1b[0m. Select one", ws_name);
        Self::select_menu(&title, &candidates, default_idx).unwrap_or_else(|| candidates[default_idx].clone())
    }

    pub fn select_interactive(target_workspace: &Path, current_profile: &str) -> Option<String> {
        let options = Self::list_available();
        if options.is_empty() {
            return None;
        }

        let ws_name = target_workspace.file_name().and_then(|n| n.to_str()).unwrap_or("workspace");

        let default_idx = options
            .iter()
            .position(|opt| opt == current_profile || opt.ends_with(&format!("/{}", current_profile)))
            .unwrap_or(0);
        let title = format!("Select Agent Profile for workspace \x1b[1;33m{}\x1b[0m", ws_name);
        Self::select_menu(&title, &options, default_idx)
    }

    fn build_rows(
        roots: &[String],
        categories: &BTreeMap<String, Vec<String>>,
        expanded: &HashMap<String, bool>,
    ) -> Vec<MenuRow> {
        let mut rows = Vec::new();
        for r in roots {
            rows.push(MenuRow::Root { name: r.clone() });
        }
        for (cat, children) in categories {
            let is_exp = expanded.get(cat).copied().unwrap_or(false);
            rows.push(MenuRow::Category { name: cat.clone(), count: children.len(), expanded: is_exp });
            if is_exp {
                for (i, child) in children.iter().enumerate() {
                    rows.push(MenuRow::Child {
                        category: cat.clone(),
                        name: child.clone(),
                        is_last: i == children.len() - 1,
                    });
                }
            }
        }
        rows
    }

    fn select_menu(prompt_title: &str, options: &[String], default_idx: usize) -> Option<String> {
        if options.is_empty() {
            return None;
        }
        if options.len() == 1 {
            return Some(options[0].clone());
        }

        let mut roots = Vec::new();
        let mut categories: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut expanded: HashMap<String, bool> = HashMap::new();

        let target_default = options.get(default_idx).cloned().unwrap_or_default();
        let default_cat = target_default.split_once('/').map(|(c, _)| c.to_string());

        for opt in options {
            if let Some((cat, name)) = opt.split_once('/') {
                categories.entry(cat.to_string()).or_default().push(name.to_string());
                let is_current = default_cat.as_deref() == Some(cat);
                expanded.entry(cat.to_string()).or_insert(is_current);
            } else {
                roots.push(opt.clone());
            }
        }

        let mut visible_rows = Self::build_rows(&roots, &categories, &expanded);
        let mut sel_idx =
            visible_rows.iter().position(|r| r.profile_id().as_deref() == Some(&target_default)).unwrap_or(0);

        let _ = enable_raw_mode();
        let _raw_guard = RawModeGuard;
        let mut stdout = io::stdout();

        print!("\x1b[?25l");
        print!("\r\x1b[2K\x1b[1;36m[ai]\x1b[0m {}:\r\n", prompt_title);
        print!("\r\x1b[2K\r\n");

        let mut first = true;
        let mut prev_row_count = visible_rows.len();

        loop {
            // Ascend exactly (prev_row_count + 1) lines back to Row 0
            if !first {
                print!("\x1b[{}A\r", prev_row_count + 1);
            }
            first = false;

            let mut top_idx = 0;
            for (idx, row) in visible_rows.iter().enumerate() {
                let is_sel = idx == sel_idx;

                match row {
                    MenuRow::Root { name } => {
                        top_idx += 1;
                        if is_sel {
                            print!("\r\x1b[2K\x1b[1;36m  ❯ \x1b[1;37m{:2}. \x1b[1;36m{}\x1b[0m\r\n", top_idx, name);
                        } else {
                            print!("\r\x1b[2K\x1b[2m    {:2}. {}\x1b[0m\r\n", top_idx, name);
                        }
                    }
                    MenuRow::Category { name, count, expanded } => {
                        top_idx += 1;
                        let icon = if *expanded { "▾" } else { "▸" };
                        if is_sel {
                            if *expanded {
                                print!(
                                    "\r\x1b[2K\x1b[1;36m  ❯ \x1b[1;37m{:2}. \x1b[1;33m{} {}\x1b[0m\r\n",
                                    top_idx, icon, name
                                );
                            } else {
                                print!(
                                    "\r\x1b[2K\x1b[1;36m  ❯ \x1b[1;37m{:2}. \x1b[1;33m{} {}\x1b[0m \x1b[2m({})\x1b[0m\r\n",
                                    top_idx, icon, name, count
                                );
                            }
                        } else {
                            if *expanded {
                                print!("\r\x1b[2K\x1b[2m    {:2}. \x1b[0;33m{} {}\x1b[0m\r\n", top_idx, icon, name);
                            } else {
                                print!(
                                    "\r\x1b[2K\x1b[2m    {:2}. \x1b[0;33m{} {}\x1b[0m \x1b[2m({})\x1b[0m\r\n",
                                    top_idx, icon, name, count
                                );
                            }
                        }
                    }
                    MenuRow::Child { name, is_last, .. } => {
                        let branch = if *is_last { "└─" } else { "├─" };
                        if is_sel {
                            print!(
                                "\r\x1b[2K\x1b[1;36m  ❯\x1b[0m       \x1b[1;37m{} \x1b[1;36m{}\x1b[0m\r\n",
                                branch, name
                            );
                        } else {
                            print!("\r\x1b[2K\x1b[2m          {} {}\x1b[0m\r\n", branch, name);
                        }
                    }
                }
            }

            print!("\r\x1b[2K\r\n");
            // Footer + \x1b[J cleanly purges old lines beneath when collapsing
            print!("\r\x1b[2K  \x1b[2m:: Enter: select/fold | Space: fold | ↑/↓: navigate | Esc: default\x1b[0m\x1b[J");
            let _ = stdout.flush();

            prev_row_count = visible_rows.len();

            let ev = match event::read() {
                Ok(e) => e,
                Err(_) => return None,
            };
            if let Event::Key(key_event) = ev {
                if key_event.kind != KeyEventKind::Press {
                    continue;
                }

                let num_rows = visible_rows.len();

                match key_event.code {
                    KeyCode::Up => {
                        sel_idx = (sel_idx + num_rows - 1) % num_rows;
                    }
                    KeyCode::Down => {
                        sel_idx = (sel_idx + 1) % num_rows;
                    }
                    KeyCode::Enter => match &visible_rows[sel_idx] {
                        MenuRow::Root { .. } | MenuRow::Child { .. } => {
                            let chosen = visible_rows[sel_idx].profile_id();
                            print!("\x1b[{}A\r\x1b[J\x1b[?25h", prev_row_count + 3);
                            let _ = stdout.flush();
                            let _ = disable_raw_mode();
                            return chosen;
                        }
                        MenuRow::Category { name, .. } => {
                            let cur = expanded.entry(name.clone()).or_insert(false);
                            *cur = !*cur;
                            visible_rows = Self::build_rows(&roots, &categories, &expanded);
                            sel_idx = sel_idx.min(visible_rows.len() - 1);
                        }
                    },
                    KeyCode::Char(' ') => {
                        if let MenuRow::Category { name, .. } = &visible_rows[sel_idx] {
                            let cur = expanded.entry(name.clone()).or_insert(false);
                            *cur = !*cur;
                            visible_rows = Self::build_rows(&roots, &categories, &expanded);
                            sel_idx = sel_idx.min(visible_rows.len() - 1);
                        }
                    }
                    KeyCode::Right => {
                        if let MenuRow::Category { name, expanded: is_exp, .. } = &visible_rows[sel_idx] {
                            if !*is_exp {
                                expanded.insert(name.clone(), true);
                                visible_rows = Self::build_rows(&roots, &categories, &expanded);
                            } else if sel_idx + 1 < visible_rows.len() {
                                sel_idx += 1;
                            }
                        }
                    }
                    KeyCode::Left => match &visible_rows[sel_idx] {
                        MenuRow::Category { name, expanded: is_exp, .. } if *is_exp => {
                            expanded.insert(name.clone(), false);
                            visible_rows = Self::build_rows(&roots, &categories, &expanded);
                        }
                        MenuRow::Child { category, .. } => {
                            let cat_name = category.clone();
                            expanded.insert(cat_name.clone(), false);
                            visible_rows = Self::build_rows(&roots, &categories, &expanded);
                            sel_idx = visible_rows
                                .iter()
                                .position(|r| matches!(r, MenuRow::Category { name, .. } if name == &cat_name))
                                .unwrap_or(0);
                        }
                        _ => {}
                    },
                    KeyCode::Esc | KeyCode::Char('q') => {
                        print!("\x1b[{}A\r\x1b[J\x1b[?25h", prev_row_count + 3);
                        let _ = stdout.flush();
                        let _ = disable_raw_mode();
                        return None;
                    }
                    KeyCode::Char('c') if key_event.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        print!("\x1b[{}A\r\x1b[J\x1b[?25h", prev_row_count + 3);
                        let _ = stdout.flush();
                        let _ = disable_raw_mode();
                        return None;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn ensure_model_workspace(provider: &crate::config::ProviderConfig) -> Option<String> {
        let is_local = crate::client::is_local_endpoint(&provider.base_url);
        let root = dirs::home_dir()?.join(".config/rs-agent");
        ensure_workspace_in_root(&root, &provider.model, is_local, true)
    }
}

fn is_param_size_tok(tok: &str) -> bool {
    let t = tok.trim_start_matches('a');
    if let Some(num) = t.strip_suffix('b') {
        !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.')
    } else {
        false
    }
}

fn is_noise_tok(tok: &str) -> bool {
    is_param_size_tok(tok)
        || matches!(
            tok,
            "q4" | "q5" | "q6" | "q8" | "iq4" | "fp16" | "bf16" | "gguf" | "instruct" | "it" | "chat"
                | "preview" | "abliterated" | "distill" | "apex" | "miniplus" | "nanoplus" | "compact"
                | "free" | "base" | "mtp" | "qad" | "versatile" | "hydrogen" | "flash" | "lite" | "mini"
                | "tiny" | "ultra" | "lightning" | "coder" | "sonnet" | "opus" | "haiku" | "pro" | "plus"
                | "i" | "v"
        )
}

fn extract_version_digits(tok: &str) -> Option<String> {
    if is_param_size_tok(tok) || is_noise_tok(tok) {
        return None;
    }
    let clean = tok.strip_prefix('v').unwrap_or(tok);
    let trimmed = clean.strip_suffix(".0").unwrap_or(clean);
    if trimmed == "4o" {
        return Some("4o".to_string());
    }
    if trimmed.is_empty() || trimmed.len() > 4 || !trimmed.chars().next()?.is_ascii_digit() {
        return None;
    }
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    if trimmed.chars().all(|c| c.is_ascii_digit() || c == '.') && !digits.is_empty() && digits.len() <= 3 {
        Some(digits)
    } else {
        None
    }
}

pub fn model_slug(raw: &str) -> String {
    let tail = raw.trim().rsplit('/').next().unwrap_or("");
    let before_tag = tail.split(':').next().unwrap_or("");
    let stripped = before_tag.strip_suffix(".gguf").or_else(|| before_tag.strip_suffix(".GGUF")).unwrap_or(before_tag);

    let tokens: Vec<String> = stripped
        .split(['-', '_', ' '])
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let mut idx = tokens.iter().position(|t| !is_noise_tok(t)).unwrap_or(0);
    let Some(first) = tokens.get(idx) else {
        return String::new();
    };
    let clean_first = first.strip_suffix(".0").unwrap_or(first);
    let mut slug: String = clean_first.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    idx += 1;

    if !slug.chars().any(|c| c.is_ascii_digit()) {
        let mut ver_added = 0;
        while idx < tokens.len() && ver_added < 2 {
            let tok = &tokens[idx];
            if tok == "v" {
                idx += 1;
                continue;
            }
            if let Some(ver) = extract_version_digits(tok) {
                if slug.len() + ver.len() <= 10 {
                    slug.push_str(&ver);
                }
                ver_added += 1;
                idx += 1;
            } else {
                break;
            }
        }
    }

    slug.chars().take(10).collect()
}

fn command_shadows_shell(cmd: &str) -> bool {
    const BUILTINS: &[&str] = &[
        "alias", "bg", "cd", "echo", "eval", "exec", "exit", "export", "false", "fc", "fg", "hash", "jobs",
        "kill", "local", "printf", "pwd", "read", "set", "shift", "shopt", "test", "times", "trap", "true",
        "type", "ulimit", "umask", "unset", "wait",
    ];
    if BUILTINS.contains(&cmd) {
        return true;
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        if dir.ends_with("rs-agent/target/release") {
            continue;
        }
        let cand = dir.join(cmd);
        if let Ok(meta) = fs::metadata(&cand) {
            if meta.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if meta.permissions().mode() & 0o111 != 0 {
                        return true;
                    }
                }
                #[cfg(not(unix))]
                return true;
            }
        }
    }
    false
}

pub fn pick_short_alias(slug: &str, used_aliases: &HashSet<String>, check_path: bool) -> Option<String> {
    let alnum: String = slug.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if alnum.len() <= 3 {
        return None;
    }
    for len in 3..alnum.len() {
        let cand = &alnum[..len];
        if cand != slug && !used_aliases.contains(cand) && (!check_path || !command_shadows_shell(cand)) {
            return Some(cand.to_string());
        }
    }
    None
}

pub fn build_auto_profile_content(model: &str, is_local: bool) -> String {
    if is_local {
        format!(
            "---\ndescription: \"{model} (Local)\"\nadapters: true\nreasoning_budget: 500\n---\nROLE: {model} Engineer.\n- Greetings/chat: 1 sentence, 0 tools, minimal thought.\n- Tools: Emit directly, no preamble/filler, relative paths only.\n- Inspect (`list_dir`/`search_code`/`read_file`) -> Edit (`edit_file`/`write_file`). Never `read_file` after own write/edit.\n- Run `run_command` only if tests requested; never repeat identical failing command without edit.\n- Halt: `✓ Task complete: <summary>`\n"
        )
    } else {
        format!(
            "---\ndescription: \"{model} (Cloud)\"\ncore4: true\nctx: 65536\nreasoning_budget: medium\n---\nROLE: {model} Engineer.\n- Greetings/questions: Reply directly (1 sentence for greetings), 0 tools/commands, minimal thought.\n- Tasks: Parallel `read_file` -> `edit_file`/`write_file` -> `run_command` tests. No filler.\n- Halt: `✓ Task complete: <summary>`\n"
        )
    }
}

fn read_profile_description(prof_path: &Path) -> Option<String> {
    let content = fs::read_to_string(prof_path).ok()?;
    let mut in_fm = false;
    for line in content.lines() {
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
                if k.trim() == "description" {
                    return Some(strip_inline_comment(v.trim()).trim_matches(['"', '\'']).to_string());
                }
            }
        }
    }
    None
}

fn find_profile_file_in(profiles_dir: &Path, profile_name: &str) -> Option<PathBuf> {
    let direct = profiles_dir.join(format!("{profile_name}.md"));
    if direct.is_file() {
        return Some(direct);
    }
    let stem = profile_name.split('/').next_back().unwrap_or(profile_name);
    if let Ok(entries) = fs::read_dir(profiles_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let sub = entry.path().join(format!("{stem}.md"));
                if sub.is_file() {
                    return Some(sub);
                }
            }
        }
    }
    None
}

fn workspace_matches_model(
    proj_dir: &Path,
    profiles_dir: &Path,
    raw_model: &str,
    base: &str,
    is_local: bool,
) -> bool {
    let Some(dir_name) = proj_dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let cfg = crate::config::WorkspaceConfig::load_or_default(proj_dir);
    let desc = find_profile_file_in(profiles_dir, &cfg.profile_name)
        .and_then(|p| read_profile_description(&p))
        .unwrap_or_default()
        .to_ascii_lowercase();

    if desc.ends_with("(local)") && !is_local {
        return false;
    }
    if desc.ends_with("(cloud)") && is_local {
        return false;
    }

    let model_low = raw_model.to_ascii_lowercase();
    if !desc.is_empty() && desc.contains(&model_low) && (desc.ends_with("(local)") || desc.ends_with("(cloud)") || is_local == cfg.adapters) {
        return true;
    }
    if is_local == cfg.adapters {
        if dir_name.eq_ignore_ascii_case(base) {
            return true;
        }
        let candidates = vec![dir_name.to_string(), cfg.profile_name.clone()];
        if !Profile::find_model_candidates(raw_model, &candidates).is_empty() {
            return true;
        }
    }
    false
}

fn has_project_route(ctx_content: &str, slug: &str) -> bool {
    let needle = format!("projects/{slug}");
    ctx_content.lines().any(|line| {
        if let Some(pos) = line.find(&needle) {
            let after = &line[pos + needle.len()..];
            after.is_empty() || after.starts_with([' ', '\t', '-'])
        } else {
            false
        }
    })
}

pub fn ensure_workspace_in_root(
    root: &Path,
    raw_model: &str,
    is_local: bool,
    check_path: bool,
) -> Option<String> {
    let model = raw_model.trim();
    if model.is_empty() || model.eq_ignore_ascii_case("local-model") || model.eq_ignore_ascii_case("default") {
        return None;
    }

    let base = model_slug(model);
    if base.is_empty() {
        return None;
    }

    let projects_dir = root.join("projects");
    let profiles_dir = root.join("profiles");

    if let Ok(entries) = fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name == "chat" || name.starts_with('.') {
                continue;
            }
            if workspace_matches_model(&p, &profiles_dir, model, &base, is_local) {
                return None;
            }
        }
    }

    let mut candidates = vec![base.clone(), format!("{base}-{}", if is_local { 'l' } else { 'c' })];
    for n in 2..=4 {
        candidates.push(format!("{base}-{n}"));
    }

    let slug = candidates.into_iter().find(|c| {
        let proj_exists = projects_dir.join(c).exists();
        if proj_exists {
            return false;
        }
        // Avoid reusing a profile file created for the opposite mode (local vs cloud)
        if let Some(desc) = read_profile_description(&profiles_dir.join(format!("{c}.md"))) {
            let low = desc.to_ascii_lowercase();
            if (is_local && low.ends_with("(cloud)")) || (!is_local && low.ends_with("(local)")) {
                return false;
            }
        }
        true
    })?;

    let proj_dir = projects_dir.join(&slug);
    let cfg = crate::config::WorkspaceConfig {
        profile_name: slug.clone(),
        is_agent: true,
        tier: if is_local { ToolTier::Smol } else { ToolTier::Core4 },
        adapters: is_local,
        delegate: false,
    };
    cfg.save(&proj_dir).ok()?;

    let _ = fs::create_dir_all(&profiles_dir);
    let prof_path = profiles_dir.join(format!("{slug}.md"));
    if !prof_path.exists() {
        let _ = fs::write(&prof_path, build_auto_profile_content(model, is_local));
    }

    let ctx_path = root.join("rs-context.md");
    let ctx_content = fs::read_to_string(&ctx_path).unwrap_or_default();
    if !has_project_route(&ctx_content, &slug) {
        let mut used: HashSet<String> = HashSet::new();
        for line in ctx_content.lines() {
            if let Some(entry) = crate::matcher::ContextEntry::parse(line) {
                used.extend(entry.aliases);
            }
        }
        let aliases = match pick_short_alias(&slug, &used, check_path) {
            Some(short) => format!("{slug}, {short}"),
            None => slug.clone(),
        };
        let route_line = format!("ai init ~/.config/rs-agent/projects/{slug} ---> {aliases}");
        let mut out_lines = Vec::new();
        let mut inserted = false;
        for line in ctx_content.lines() {
            out_lines.push(line.to_string());
            if !inserted && line.trim() == "# --- Workspaces ---" {
                out_lines.push(route_line.clone());
                inserted = true;
            }
        }
        if !inserted {
            out_lines.push(route_line);
        }
        let tmp_ctx = root.join(format!("rs-context.md.tmp.{}", std::process::id()));
        if fs::write(&tmp_ctx, out_lines.join("\n") + "\n").is_ok() {
            let _ = fs::rename(&tmp_ctx, &ctx_path);
        }
    }

    Some(slug)
}

pub fn build_system_prompt(profile: &Profile, workspace: &Path, is_agent: bool, active_skills: &[String]) -> String {
    let mut prompt = if is_agent {
        format!("{}\n\nWorkspace Path: {}", profile.system_prompt, workspace.display())
    } else {
        profile.system_prompt.clone()
    };

    if let Some(global_rules) = crate::config::load_global_instructions() {
        prompt.push_str(&format!("\n\n---\nGLOBAL SYSTEM INSTRUCTIONS:\n{}", global_rules));
    }

    if is_agent && profile.memory {
        let mem_path = workspace.join(".rs/memory.md");
        if let Ok(c) = fs::read_to_string(mem_path) {
            let trimmed = c.trim();
            if !trimmed.is_empty() {
                prompt.push_str(&format!("\n\n---\nWORKSPACE MEMORY (.rs/memory.md):\n{}", trimmed));
            }
        }
    }

    for sk_name in active_skills {
        if let Some(sk) = crate::skill::Skill::load(sk_name) {
            prompt.push_str(&format!("\n\n---\nSKILL DIRECTIVE [{}]:\n{}", sk.name, sk.content));
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_parses_cloud_effort_and_core4() {
        let raw = "---\ncore4: true # minimal tools\nreasoning_budget: xhigh # cloud tier\nadapters: true\n---\nSystem prompt";
        let prof = Profile::parse(raw);
        assert!(matches!(prof.tier, ToolTier::Core4));
        assert_eq!(prof.reasoning_budget, Some(ReasoningSetting::Tier("xhigh".into())));
        assert!(prof.adapters);
    }

    #[test]
    fn test_google_cloud_tier_clamping() {
        assert_eq!(ReasoningSetting::Tokens(0).to_cloud_tier(true, "gemini-3.5-flash-lite"), "minimal");
        assert_eq!(ReasoningSetting::Tier("xhigh".into()).to_cloud_tier(true, "gemini-3.5-flash-lite"), "high");
        assert_eq!(ReasoningSetting::Tier("xhigh".into()).to_cloud_tier(true, "gemini-4-hydrogen"), "xhigh");
        assert_eq!(ReasoningSetting::Tier("none".into()).to_cloud_tier(true, "gemini-4-hydrogen"), "low");
    }

    #[test]
    fn test_reasoning_budget_parses_tokens_and_tiers() {
        let local = Profile::parse("---\nreasoning_budget: 500\n---\nhi");
        assert_eq!(local.reasoning_budget, Some(ReasoningSetting::Tokens(500)));
        let cloud = Profile::parse("---\nreasoning_budget: high\n---\nhi");
        assert_eq!(cloud.reasoning_budget, Some(ReasoningSetting::Tier("high".into())));
    }

    #[test]
    fn test_hud_summary_formats_frontmatter() {
        let ling = Profile::parse("---\nadapters: true\nreasoning_budget: 350\n---\nhi");
        assert_eq!(ling.hud_summary(true), "350 · adp");

        let gem = Profile::parse("---\ncore4: true\nreasoning_budget: xhigh\nmemory: true\ndelegate: true\nmcp: photocraft\n---\nhi");
        assert_eq!(gem.hud_summary(true), "core4 · xhigh · mem · del · mcp:photocraft");

        let chat = Profile::parse("hi");
        assert_eq!(chat.hud_summary(false), "no tools · pure chat");
    }

    #[test]
    fn test_find_workspace_candidates_resolves_ling_and_exact_groot() {
        let avail = vec![
            "artcraft/photocraft-c".to_string(),
            "artcraft/photocraft-l".to_string(),
            "groot/groot".to_string(),
            "groot/groot2".to_string(),
            "lfm3".to_string(),
            "lingtiny".to_string(),
        ];
        assert_eq!(Profile::find_workspace_candidates("lin", &avail), vec!["lingtiny"]);
        assert_eq!(Profile::find_workspace_candidates("ling", &avail), vec!["lingtiny"]);
        assert_eq!(Profile::find_workspace_candidates("ling-tiny", &avail), vec!["lingtiny"]);
        assert_eq!(Profile::find_workspace_candidates("lfm", &avail), vec!["lfm3"]);
        assert_eq!(Profile::find_workspace_candidates("groot", &avail), vec!["groot/groot"]);
        assert_eq!(
            Profile::find_workspace_candidates("artcraft", &avail),
            vec!["artcraft/photocraft-c", "artcraft/photocraft-l"]
        );
    }

    #[test]
    fn test_profile_parses_mcp_tools_allowlist() {
        let p = Profile::parse("---\nmcp: photocraft\nmcp_tools: doc_new, doc_save, ui_set\n---\nhi");
        assert_eq!(p.mcp_servers, vec!["photocraft"]);
        assert_eq!(p.mcp_tools, vec!["doc_new", "doc_save", "ui_set"]);
        assert_eq!(p.hud_summary(true), "mcp:photocraft(3)");
    }

    #[test]
    fn test_profile_parses_ctx_override() {
        assert_eq!(Profile::parse("---\nctx: 65536\n---\nhi").ctx, Some(65536));
        assert_eq!(Profile::parse("---\nctx: 64k\n---\nhi").ctx, Some(65536));
        assert_eq!(Profile::parse("---\nctx: 128k\n---\nhi").ctx, Some(131072));
        assert_eq!(Profile::parse("---\nctx: 1m\n---\nhi").ctx, Some(1048576));
    }

    #[test]
    fn test_model_slug_normalizes_local_and_cloud_names() {
        assert_eq!(model_slug("Ling-3.0-tiny"), "ling3");
        assert_eq!(model_slug("LFM3-1.7B-QAD-Q4_0.gguf"), "lfm3");
        assert_eq!(model_slug("Qwen3.8-35B-A3B-Distill-MTP-APEX-I-MiniPlus-V2.1"), "qwen38");
        assert_eq!(model_slug("Gemma-4-31B-IT"), "gemma4");
        assert_eq!(model_slug("nvidia/nemotron-3.5-lightning:free"), "nemotron35");
        assert_eq!(model_slug("DeepSeek-V4.1-Flash"), "deepseek41");
        assert_eq!(model_slug("claude-3-5-sonnet-20241022"), "claude35");
        assert_eq!(model_slug("openai/gpt-4o"), "gpt4o");
        assert_eq!(model_slug("Flint-40B-A2.5B-APEX-I-NanoPlus-V2.1"), "flint");
    }

    #[test]
    fn test_pick_short_alias_steps_3_to_4_to_5_avoiding_collisions() {
        let used: HashSet<String> = ["qwe", "qwen"].into_iter().map(String::from).collect();
        assert_eq!(pick_short_alias("qwen38", &used, false).as_deref(), Some("qwen3"));
        assert_eq!(pick_short_alias("gpt", &used, false), None);
    }

    #[test]
    fn test_build_auto_profile_content_local_vs_cloud() {
        let local = build_auto_profile_content("Qwen3.8-35B", true);
        assert!(local.contains("adapters: true") && !local.contains("core4: true"));
        assert!(!local.contains("ctx:") && local.contains("reasoning_budget: 500"));
        for tool in ["list_dir", "search_code", "read_file", "edit_file", "write_file", "run_command"] {
            assert!(local.contains(tool), "missing {tool}");
        }

        let cloud = build_auto_profile_content("Gemma-4-31B-IT", false);
        assert!(cloud.contains("core4: true") && !cloud.contains("adapters: true"));
        assert!(cloud.contains("ctx: 65536") && cloud.contains("reasoning_budget: medium"));
        for tool in ["read_file", "edit_file", "write_file", "run_command"] {
            assert!(cloud.contains(tool), "missing {tool}");
        }
    }

    #[test]
    fn test_ensure_workspace_in_root_scaffolds_once_and_separates_local_cloud() {
        let tmp = std::env::temp_dir().join(format!("rs_agent_scaffold_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("rs-context.md"), "# --- Workspaces ---\n").unwrap();

        assert_eq!(ensure_workspace_in_root(&tmp, "local-model", true, false), None);

        let created = ensure_workspace_in_root(&tmp, "Gemma-4-31B-IT", true, false);
        assert_eq!(created.as_deref(), Some("gemma4"));
        assert_eq!(ensure_workspace_in_root(&tmp, "Gemma-4-31B-IT", true, false), None);

        let cloud = ensure_workspace_in_root(&tmp, "google/gemma-4-31b:free", false, false);
        assert_eq!(cloud.as_deref(), Some("gemma4-c"));

        let ctx_txt = fs::read_to_string(tmp.join("rs-context.md")).unwrap();
        assert!(ctx_txt.contains("projects/gemma4 ---> gemma4, gem"));
        assert!(ctx_txt.contains("projects/gemma4-c ---> gemma4-c, gemm"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
