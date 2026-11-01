use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

const GREEN: &str = "\x1b[1;32m";
const CYAN: &str = "\x1b[1;36m";
const YELLOW: &str = "\x1b[1;33m";
const RESET: &str = "\x1b[0m";

fn get_rs_agent_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/rs-agent")
}

fn list_profiles(root: &PathBuf) -> Vec<String> {
    let prof_dir = root.join("profiles");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(prof_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

fn main() {
    println!("\n{}=================================================={}", CYAN, RESET);
    println!("{}           rs-agent Project Creator               {}", CYAN, RESET);
    println!("{}=================================================={}\n", CYAN, RESET);

    let root = get_rs_agent_root();
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // 1. Get Project Name interactively if not passed via CLI
    let name = if !args.is_empty() {
        args.remove(0)
    } else {
        print!("{}Enter Project Name:{} ", GREEN, RESET);
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    };

    if name.is_empty() {
        eprintln!("\x1b[1;31m[error]\x1b[0m Project name cannot be empty.");
        std::process::exit(1);
    }

    let clean_name = name.to_lowercase().replace(' ', "-");
    let proj_dir = root.join("projects").join(&clean_name);

    if proj_dir.exists() {
        eprintln!("\x1b[1;33m[warn]\x1b[0m Workspace already exists at: {}", proj_dir.display());
        std::process::exit(0);
    }

    // 2. Choose profile
    let profiles = list_profiles(&root);
    let default_prof = if profiles.contains(&"lfm3".to_string()) {
        "lfm3"
    } else if !profiles.is_empty() {
        &profiles[0]
    } else {
        "chat"
    };

    println!("\n{}Available profiles:{} {}", YELLOW, RESET, profiles.join(", "));
    print!("{}Choose profile [default: {}]:{} ", GREEN, default_prof, RESET);
    io::stdout().flush().unwrap();
    let mut prof_input = String::new();
    io::stdin().read_line(&mut prof_input).unwrap();
    let chosen_prof = if prof_input.trim().is_empty() {
        default_prof.to_string()
    } else {
        prof_input.trim().to_string()
    };

    // 3. Create workspace structure
    let meta_dir = proj_dir.join(".rs");
    if let Err(e) = fs::create_dir_all(&meta_dir) {
        eprintln!("\x1b[1;31m[error]\x1b[0m Failed to create workspace dir: {}", e);
        std::process::exit(1);
    }

    // 4. Write .rs/config.json
    let config_content = format!(
        "{{\n  \"profile_name\": \"{}\",\n  \"tier\": \"Smol\",\n  \"is_agent\": true\n}}\n",
        chosen_prof
    );
    let _ = fs::write(meta_dir.join("config.json"), config_content);

    // 5. Insert directly beneath "# --- Workspaces ---" in rs-context.md
    let ctx_file = root.join("rs-context.md");
    if let Ok(content) = fs::read_to_string(&ctx_file) {
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
    println!("\x1b[1;32m✓ Configured profile:\x1b[0m {}", chosen_prof);
    println!("\x1b[1;32m✓ Added to top of Workspaces:\x1b[0m '{}' in rs-context.md", clean_name);
    println!("\n{}To start your workspace now, type:{} ai init ~/.config/rs-agent/projects/{}\n", CYAN, RESET, clean_name);
}
