use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const GREEN: &str = "\x1b[1;32m";
const CYAN: &str = "\x1b[1;36m";
const YELLOW: &str = "\x1b[1;33m";
const RESET: &str = "\x1b[0m";

fn prompt_line(prompt: &str) -> String {
    print!("{}", prompt);
    let _ = io::stdout().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return String::new();
    }
    input.trim().to_string()
}

fn get_rs_agent_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/rs-agent")
}

fn list_profiles(root: &Path) -> Vec<String> {
    let prof_dir = root.join("profiles");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(prof_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if !dir_name.starts_with('.') {
                    if let Ok(sub_entries) = fs::read_dir(&p) {
                        for sub in sub_entries.flatten() {
                            let sp = sub.path();
                            if sp.extension().and_then(|s| s.to_str()) == Some("md") {
                                if let Some(stem) = sp.file_stem().and_then(|s| s.to_str()) {
                                    if stem != "chat" {
                                        out.push(format!("{}/{}", dir_name, stem));
                                    }
                                }
                            }
                        }
                    }
                }
            } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    if stem != "chat" {
                        out.push(stem.to_string());
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| {
        let a_nested = a.contains('/');
        let b_nested = b.contains('/');
        match (a_nested, b_nested) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });
    out
}

fn normalize_profile_name(raw_name: &str) -> String {
    raw_name.trim().trim_end_matches(".md").replace(' ', "-").to_lowercase()
}

fn read_profile_flags(root: &Path, prof_name: &str) -> (&'static str, bool, bool) {
    let prof_path = root.join("profiles").join(format!("{}.md", prof_name));
    let Ok(content) = fs::read_to_string(prof_path) else {
        return ("Smol", false, false);
    };
    let mut tier = "Smol";
    let mut adapters = false;
    let mut delegate = false;
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
        if !in_fm {
            continue;
        }
        if let Some((k, v)) = t.split_once(':') {
            let key = k.trim();
            let val = v.split('#').next().unwrap_or(v).trim().trim_matches(['"', '\'']);
            match key {
                "tier" if val.eq_ignore_ascii_case("purebash") => tier = "PureBash",
                "tier" if val.eq_ignore_ascii_case("core4") => tier = "Core4",
                "purebash" if val.eq_ignore_ascii_case("true") => tier = "PureBash",
                "core4" if val.eq_ignore_ascii_case("true") => tier = "Core4",
                "adapters" => adapters = val.eq_ignore_ascii_case("true"),
                "delegate" | "subagent" | "subagents" => delegate = val.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
    }
    (tier, adapters, delegate)
}

fn create_profile_file(root: &Path, clean: &str) -> String {
    let profiles_dir = root.join("profiles");
    let _ = fs::create_dir_all(&profiles_dir);

    let target_file = profiles_dir.join(format!("{}.md", clean));
    if let Some(parent) = target_file.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if !target_file.exists() {
        let content = format!(
            "---\ndescription: \"{} Workspace Profile\"\nadapters: true\nmemory: true\n---\nROLE: Autonomous Systems & Software Engineer for {}.\n\nDIRECTIVES:\n- Inspect -> Edit -> Verify.\n- Fast, clean, minimal execution without dead code or conversational filler.\n- Stop immediately when tests pass with '✓ Task complete: <summary>'.\n",
            clean, clean
        );
        if let Err(e) = fs::write(&target_file, content) {
            eprintln!("\x1b[1;31m[warn]\x1b[0m Could not write profile file: {}", e);
        } else {
            println!("\x1b[1;32m✓ Created new profile:\x1b[0m {}", target_file.display());
        }
    } else {
        println!("\x1b[1;33m[info]\x1b[0m Profile file already exists: {}", target_file.display());
    }

    clean.to_string()
}

fn main() {
    println!("\n{}=================================================={}", CYAN, RESET);
    println!("{}           rs-agent Project Creator               {}", CYAN, RESET);
    println!("{}=================================================={}\n", CYAN, RESET);

    let root = get_rs_agent_root();
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    let name = if !args.is_empty() {
        args.remove(0)
    } else {
        prompt_line(&format!("{}Enter Project Name:{} ", GREEN, RESET))
    };

    let clean_name = name.trim().to_lowercase().replace(' ', "-");
    if clean_name.is_empty() || clean_name == "." || clean_name == ".." || clean_name.contains(['/', '\\']) {
        eprintln!("\x1b[1;31m[error]\x1b[0m Invalid project name '{}'.", name);
        std::process::exit(1);
    }

    let proj_dir = root.join("projects").join(&clean_name);
    if proj_dir.exists() {
        eprintln!("\x1b[1;33m[warn]\x1b[0m Workspace already exists at: {}", proj_dir.display());
        std::process::exit(0);
    }

    let profiles = list_profiles(&root);
    let default_prof = if profiles.iter().any(|p| p == "lfm3") {
        "lfm3"
    } else if !profiles.is_empty() {
        &profiles[0]
    } else {
        "chat"
    };

    println!("\n{}Available profiles:{} {}", YELLOW, RESET, profiles.join(", "));
    println!("{}Tip:{} Choose an existing profile, enter 'new', or type a custom profile name.", CYAN, RESET);
    let prof_input = prompt_line(&format!("{}Choose profile [default: {}]:{} ", GREEN, default_prof, RESET));
    let input_str = prof_input.as_str();

    let chosen_prof = if input_str.is_empty() {
        default_prof.to_string()
    } else if input_str == "new" || input_str == "create" {
        let new_prof_input =
            prompt_line(&format!("{}Enter name for new profile [default: {}]:{} ", GREEN, clean_name, RESET));
        let target_name = if new_prof_input.is_empty() {
            clean_name.clone()
        } else {
            normalize_profile_name(&new_prof_input)
        };
        if target_name.contains("..") || target_name.starts_with('/') {
            eprintln!("\x1b[1;31m[error]\x1b[0m Invalid profile name '{}'.", target_name);
            std::process::exit(1);
        }
        create_profile_file(&root, &target_name)
    } else if let Some(found) = profiles.iter().find(|p| *p == input_str || p.ends_with(&format!("/{}", input_str))) {
        found.clone()
    } else {
        let norm_new = normalize_profile_name(input_str);
        if norm_new.is_empty() || norm_new.contains("..") || norm_new.starts_with('/') {
            eprintln!("\x1b[1;31m[error]\x1b[0m Invalid profile name '{}'.", input_str);
            std::process::exit(1);
        }
        let conf_trim = prompt_line(&format!(
            "{}Profile '{}' does not exist. Create ~/.config/rs-agent/profiles/{}.md? [Y/n]:{} ",
            YELLOW, norm_new, norm_new, RESET
        ))
        .to_lowercase();
        if conf_trim.is_empty() || conf_trim == "y" || conf_trim == "yes" {
            create_profile_file(&root, &norm_new)
        } else {
            println!("{}Falling back to default profile: {}{}", YELLOW, default_prof, RESET);
            default_prof.to_string()
        }
    };

    let meta_dir = proj_dir.join(".rs");
    if let Err(e) = fs::create_dir_all(&meta_dir) {
        eprintln!("\x1b[1;31m[error]\x1b[0m Failed to create workspace dir: {}", e);
        std::process::exit(1);
    }

    let (tier, adapters, delegate) = read_profile_flags(&root, &chosen_prof);
    let config_content = format!(
        "{{\n  \"profile_name\": \"{}\",\n  \"is_agent\": true,\n  \"tier\": \"{}\",\n  \"adapters\": {},\n  \"delegate\": {}\n}}\n",
        chosen_prof, tier, adapters, delegate
    );
    let _ = fs::write(meta_dir.join("config.json"), config_content);

    let ctx_file = root.join("rs-context.md");
    let content = fs::read_to_string(&ctx_file).unwrap_or_default();
    let marker = format!("projects/{}", clean_name);
    if !content.contains(&marker) {
        let route_line = format!("ai init ~/.config/rs-agent/projects/{} ---> {}", clean_name, clean_name);
        let mut new_lines = Vec::new();
        let mut inserted = false;

        for line in content.lines() {
            new_lines.push(line.to_string());
            if !inserted && line.trim() == "# --- Workspaces ---" {
                new_lines.push(route_line.clone());
                inserted = true;
            }
        }
        if !inserted {
            new_lines.push(route_line);
        }

        let _ = fs::write(&ctx_file, new_lines.join("\n") + "\n");
    }

    println!("\n\x1b[1;32m✓ Created workspace:\x1b[0m {}", proj_dir.display());
    println!("\x1b[1;32m✓ Configured profile:\x1b[0m {} (tier={}, adapters={}, delegate={})", chosen_prof, tier, adapters, delegate);
    println!("\x1b[1;32m✓ Added to top of Workspaces:\x1b[0m '{}' in rs-context.md", clean_name);
    println!(
        "\n{}To start your workspace now, type:{} ai init ~/.config/rs-agent/projects/{}\n",
        CYAN, RESET, clean_name
    );
}
