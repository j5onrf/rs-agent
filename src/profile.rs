use crate::config::ToolTier;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Profile {
    pub reasoning_budget: Option<usize>,
    pub tier: ToolTier,
    pub adapters: bool,
    pub memory: bool,
    pub delegate: bool,
    pub system_prompt: String,
    pub mcp_servers: Vec<String>,
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
                    let val = v.trim();
                    match key {
                        "reasoning_budget" => {
                            reasoning_budget = val.parse().ok();
                        }
                        "tier" if val.eq_ignore_ascii_case("purebash") => {
                            tier = ToolTier::PureBash;
                        }
                        "purebash" if val.eq_ignore_ascii_case("true") => {
                            tier = ToolTier::PureBash;
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
                        _ => {}
                    }
                }
            } else {
                body.push_str(line);
                body.push('\n');
            }
        }

        Self { reasoning_budget, tier, adapters, memory, delegate, system_prompt: body.trim().to_string(), mcp_servers }
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
        if let Some(url) = base_url {
            let u = url.to_lowercase();
            if u.contains("localhost") || u.contains("127.0.0.1") || u.contains(":8080") || u.contains(":11434") {
                return true;
            }
        }
        let m = model_name.to_lowercase();
        m.contains("local") || m.contains("llama-server") || m.contains("lfm") || m.contains("gguf")
    }

    pub fn find_workspace_candidates(ws_name: &str, available: &[String]) -> Vec<String> {
        let ws_clean = ws_name.replace(['-', '_'], "").to_lowercase();
        let mut matches = Vec::new();

        for opt in available {
            let bare = opt.split('/').next_back().unwrap_or(opt);
            let opt_clean = opt.replace(['-', '_'], "").to_lowercase();
            let bare_clean = bare.replace(['-', '_'], "").to_lowercase();

            if opt_clean == ws_clean || bare_clean == ws_clean {
                matches.push(opt.clone());
                continue;
            }
            for sep in ['-', '_'] {
                let prefix = format!("{}{}", ws_name.to_lowercase(), sep);
                if (opt.to_lowercase().starts_with(&prefix) || bare.to_lowercase().starts_with(&prefix))
                    && !matches.contains(opt)
                {
                    matches.push(opt.clone());
                }
            }
            if (opt_clean.starts_with(&ws_clean) || bare_clean.starts_with(&ws_clean)) && !matches.contains(opt) {
                matches.push(opt.clone());
            }
        }
        matches
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

            if let Ok(Event::Key(key_event)) = event::read() {
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
}
