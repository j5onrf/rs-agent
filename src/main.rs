#![allow(dead_code)]

mod adapter;
mod client;
mod config;
mod matcher;
mod profile;
mod security;
mod skill;
mod spinner;
mod state;
mod stream;
mod tools;
mod ui;

use clap::{Parser, Subcommand};
use client::{ChatMessage, LlmClient};
use config::{ProviderConfig, WorkspaceConfig};
use matcher::ContextRouter;
use profile::Profile;
use security::SecurityBoundary;
use skill::{Skill, SkillAction};
use spinner::InlineSpinner;
use stream::StreamRenderer;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "rs-agent", about = "Blazing-fast minimal Rust AI Agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long)]
    bridge: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Init {
        path: Option<String>,
    },
    Match {
        query: Vec<String>,
    },
    New {
        name: String,
        intents: Option<String>,
    },
    Handoff,
}

fn expand_path(p: &str) -> PathBuf {
    if p == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    } else if let Some(stripped) = p.strip_prefix("~/") {
        dirs::home_dir().map(|h| h.join(stripped)).unwrap_or_else(|| PathBuf::from(p))
    } else {
        PathBuf::from(p)
    }
}

fn handle_new_project(name: &str, intents_arg: Option<&str>) -> anyhow::Result<()> {
    let slug = name.trim().to_lowercase().replace(' ', "-");
    let base_cfg = dirs::home_dir()
        .map(|h| h.join(".config/rs-agent"))
        .unwrap_or_else(|| PathBuf::from("."));

    let projects_dir = base_cfg.join("projects").join(&slug);
    fs::create_dir_all(&projects_dir)?;
    println!("\x1b[1;32m✓ Created workspace:\x1b[0m {}", projects_dir.display());

    let profiles_dir = base_cfg.join("profiles");
    fs::create_dir_all(&profiles_dir)?;
    let profile_file = profiles_dir.join(format!("{}.md", slug));
    if !profile_file.exists() {
        let content = format!(
            "---\ndescription: \"{} Workspace Profile\"\nyolo: true\nreasoning_budget: 500\n---\nROLE: Autonomous Systems & Software Engineer for {}.\n\nDIRECTIVES:\n- Inspect -> Edit -> Verify.\n- Stop immediately when tests pass with '✓ Task complete: <summary>'.\n",
            slug, slug
        );
        fs::write(&profile_file, content)?;
        println!("\x1b[1;32m✓ Created profile:\x1b[0m {}", profile_file.display());
    }

    let ctx_file = base_cfg.join("rs-context.md");
    let clean_intents = intents_arg.unwrap_or(&slug);
    let entry = format!("ai init ~/.config/rs-agent/projects/{} ---> {}\n", slug, clean_intents);

    let mut current_ctx = fs::read_to_string(&ctx_file).unwrap_or_default();
    if !current_ctx.contains(&format!("projects/{}", slug)) {
        if let Some(pos) = current_ctx.find("# --- Workspaces ---") {
            let insert_pos = pos + "# --- Workspaces ---".len();
            current_ctx.insert_str(insert_pos, &format!("\n{}", entry.trim_end()));
        } else {
            current_ctx.push_str(&entry);
        }
        fs::write(&ctx_file, current_ctx)?;
        println!("\x1b[1;32m✓ Added to rs-context.md:\x1b[0m \x1b[36m{}\x1b[0m \x1b[2m--->\x1b[0m \x1b[33m{}\x1b[0m", slug, clean_intents);
    }

    println!("\n\x1b[2mLaunch workspace:\x1b[0m \x1b[1;32mtype '{}' in terminal\x1b[0m\n", slug);
    Ok(())
}

fn get_styled_shell_prompt() -> String {
    "\x1b[33m~\x1b[0m \x1b[1;33m❯\x1b[0m ".to_string()
}

fn build_system_prompt(
    is_agent: bool,
    target_dir: &Path,
    active_profile: &Option<Profile>,
    active_skills: &[String],
) -> String {
    let mut prompt = if is_agent {
        if let Some(prof) = active_profile {
            format!("{}\n\nWorkspace Path: {}", prof.system_prompt, target_dir.display())
        } else {
            format!("You are an autonomous agent working in workspace: {}.", target_dir.display())
        }
    } else {
        if let Some(prof) = active_profile {
            prof.system_prompt.clone()
        } else {
            "You are an articulate, perceptive, and intellectually honest conversational partner.".to_string()
        }
    };

    if let Some(global_rules) = config::load_global_instructions() {
        prompt.push_str(&format!("\n\n---\nGLOBAL SYSTEM INSTRUCTIONS:\n{}", global_rules));
    }

    for sk_name in active_skills {
        if let Some(sk) = Skill::load(sk_name) {
            prompt.push_str(&format!("\n\n---\nSKILL DIRECTIVE [{}]:\n{}", sk.name, sk.content));
        }
    }

    prompt
}

async fn run_session(target_dir: &Path, is_agent_init: bool) -> anyhow::Result<()> {
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir)?;
    }
    let _ = env::set_current_dir(target_dir);

    let provider = ProviderConfig::load().await;
    let client = LlmClient::new(provider.clone());
    let boundary = SecurityBoundary::new(target_dir);

    let mut cfg = if is_agent_init {
        let mut c = WorkspaceConfig::load_or_default(target_dir);
        c.is_agent = true;

        let dir_name = target_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("agent")
            .to_string();

        c.profile_name = Profile::resolve_for_workspace(&dir_name, &provider.model, Some(&provider.base_url));

        if let Some(p) = Profile::load(&c.profile_name) {
            c.tier = p.tier.clone();
            c.yolo = p.yolo;
            c.adapters = p.adapters;
            c.thinking_budget = p.thinking_budget;
        } else {
            c.yolo = true;
        }

        let _ = c.save(target_dir);
        c
    } else {
        WorkspaceConfig::load_or_default(target_dir)
    };

    let mut active_profile = if cfg.is_agent {
        Profile::load(&cfg.profile_name)
    } else {
        Profile::load("chat")
    };

    let mut active_skills: Vec<String> = Vec::new();
    let second_line = if cfg.is_agent { cfg.profile_name.clone() } else { "chat".to_string() };

    // Initial boot: draws directly under your actual shell prompt (untouched, no screen clear)
    let mut current_box_style = state::get_box_style();
    let mut buffered_char = ui::draw_box(&provider.model, &second_line, current_box_style, true);

    let mut system_prompt = build_system_prompt(cfg.is_agent, target_dir, &active_profile, &active_skills);
    let mut history = vec![ChatMessage::system(&system_prompt)];
    let mut active_tools = if cfg.is_agent {
        Some(tools::get_tools_schema(&cfg.tier))
    } else {
        None
    };

    let mut spinner = InlineSpinner::new();

    loop {
        print!("❯ ");
        if let Some(c) = buffered_char.take() {
            print!("{}", c);
        }
        io::stdout().flush()?;

        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            break;
        }
        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        if input == "q" || input == "exit" {
            println!("Exiting session.");
            break;
        }

        // Live /p Profile switcher: resets session cleanly with fresh dynamic prompt
        if input == "/p" || input == "/profile" {
            print!("\x1b[1A\r\x1b[2K");
            let _ = io::stdout().flush();

            if let Some(new_prof) = Profile::select_interactive(target_dir, &cfg.profile_name) {
                cfg.profile_name = new_prof;
                active_profile = Profile::load(&cfg.profile_name);

                if let Some(p) = &active_profile {
                    cfg.tier = p.tier.clone();
                    cfg.yolo = p.yolo;
                    cfg.adapters = p.adapters;
                    cfg.thinking_budget = p.thinking_budget;
                }

                system_prompt = build_system_prompt(cfg.is_agent, target_dir, &active_profile, &active_skills);
                let _ = cfg.save(target_dir);

                history.clear();
                history.push(ChatMessage::system(&system_prompt));
                active_tools = if cfg.is_agent {
                    Some(tools::get_tools_schema(&cfg.tier))
                } else {
                    None
                };

                // Clean screen reset to ~ ❯ <new_profile>
                print!("\x1b[H\x1b[2J");
                let prompt = get_styled_shell_prompt();
                println!("{}{}", prompt, cfg.profile_name);
                ui::draw_box(&provider.model, &cfg.profile_name, current_box_style, false);
                let _ = io::stdout().flush();
            }
            continue;
        }

        // Live /s or /skill Skill Manager (inline confirmation, never touches the box)
        if input == "/s" || input == "/skill" || input.starts_with("/s ") || input.starts_with("/skill ") {
            let direct_arg = if input.starts_with("/s ") {
                input[3..].trim().to_lowercase()
            } else if input.starts_with("/skill ") {
                input[7..].trim().to_lowercase()
            } else {
                String::new()
            };

            print!("\x1b[1A\r\x1b[2K");
            let _ = io::stdout().flush();

            let action = if !direct_arg.is_empty() {
                if direct_arg == "clear" || direct_arg == "none" || direct_arg == "reset" {
                    Some(SkillAction::DetachAll)
                } else if active_skills.contains(&direct_arg) {
                    Some(SkillAction::Detach(direct_arg))
                } else {
                    Some(SkillAction::Attach(direct_arg))
                }
            } else {
                Skill::select_interactive(&active_skills)
            };

            if let Some(act) = action {
                match act {
                    SkillAction::Attach(skill_name) => {
                        if !active_skills.contains(&skill_name) && active_skills.len() < 3 {
                            if let Some(sk) = Skill::load(&skill_name) {
                                active_skills.push(sk.name.clone());
                                println!("  \x1b[1;32m✓\x1b[0m \x1b[2mSkill attached:\x1b[0m \x1b[1;36m{}\x1b[0m\n", sk.name);
                            }
                        } else if active_skills.len() >= 3 {
                            println!("  \x1b[1;31m✗\x1b[0m \x1b[2mMax 3 skills reached (detach one first)\x1b[0m\n");
                        }
                    }
                    SkillAction::Detach(skill_name) => {
                        if let Some(pos) = active_skills.iter().position(|x| x == &skill_name) {
                            active_skills.remove(pos);
                            println!("  \x1b[1;33m✗\x1b[0m \x1b[2mSkill detached:\x1b[0m \x1b[1;33m{}\x1b[0m\n", skill_name);
                        }
                    }
                    SkillAction::DetachAll => {
                        active_skills.clear();
                        println!("  \x1b[1;33m✗\x1b[0m \x1b[2mAll skills detached\x1b[0m\n");
                    }
                }

                system_prompt = build_system_prompt(cfg.is_agent, target_dir, &active_profile, &active_skills);
                if let Some(first) = history.first_mut() {
                    first.content = system_prompt.clone();
                }
                let _ = io::stdout().flush();
            }
            continue;
        }

        if input == "/yolo" || input == "/g" {
            cfg.yolo = !cfg.yolo;
            let _ = cfg.save(target_dir);
            let status = if cfg.yolo { "\x1b[1;32mon\x1b[0m" } else { "\x1b[1;33moff\x1b[0m" };
            println!("  \x1b[2m[yolo: {}]\x1b[0m\n", status);
            continue;
        }

        if input.starts_with("/box") {
            let parts: Vec<&str> = input.split_whitespace().collect();
            current_box_style = if parts.len() > 1 {
                parts[1].parse::<usize>().unwrap_or((current_box_style % 3) + 1)
            } else {
                (current_box_style % 3) + 1
            };
            state::set_box_style(current_box_style);

            print!("\x1b[H\x1b[2J");
            let prompt = get_styled_shell_prompt();
            let cmd = if cfg.is_agent { &cfg.profile_name } else { "ai" };
            println!("{}{}", prompt, cmd);
            let second_line = if cfg.is_agent { &cfg.profile_name } else { "chat" };
            ui::draw_box(&provider.model, second_line, current_box_style, false);
            continue;
        }

        if input == "/clear" || input == "/c" {
            history.truncate(1);
            print!("\x1b[H\x1b[2J");
            let prompt = get_styled_shell_prompt();
            let cmd = if cfg.is_agent { &cfg.profile_name } else { "ai" };
            println!("{}{}", prompt, cmd);
            let second_line = if cfg.is_agent { &cfg.profile_name } else { "chat" };
            ui::draw_box(&provider.model, second_line, current_box_style, false);
            continue;
        }

        println!();
        io::stdout().flush()?;

        history.push(ChatMessage::user(input));

        let max_rounds = 10;
        for round in 0..max_rounds {
            let prompt_chars: usize = history.iter().map(|m| m.content.len()).sum();
            let mut renderer = StreamRenderer::new();

            if round == 0 && history.len() <= 2 {
                spinner.start("Preloading...");
            } else if round == 0 {
                spinner.start("Working...");
            } else {
                spinner.start("Evaluating tool results...");
            }

            let (reply, tool_calls, stats) = client.stream_chat(
                &history,
                active_tools.clone(),
                &mut renderer,
                &mut spinner,
            ).await?;

            spinner.stop();

            if tool_calls.is_none() || tool_calls.as_ref().map(|v| v.is_empty()).unwrap_or(true) {
                renderer.finish(&stats, prompt_chars);

                // Dedicated active skills line directly under the stats line
                if !active_skills.is_empty() {
                    let formatted_skills = active_skills.join(" · ");
                    println!("  \x1b[2m{}\x1b[0m\n", formatted_skills);
                }

                history.push(ChatMessage::assistant(reply));
                break;
            }

            let calls = tool_calls.unwrap();
            let mut assistant_msg = ChatMessage::assistant(reply);
            assistant_msg.tool_calls = Some(serde_json::Value::Array(calls.clone()));
            history.push(assistant_msg);

            for call in calls {
                let call_id = call["id"].as_str().unwrap_or("call_0").to_string();
                let fn_name = call["function"]["name"].as_str().unwrap_or("tool");
                let fn_args = call["function"]["arguments"].as_str().unwrap_or("{}");

                let verb = match fn_name {
                    "write_file" => "updating",
                    "edit_file" => "surgically editing",
                    "read_file" => "checking",
                    "list_dir" => "checking",
                    "search_code" => "searching codebase",
                    "run_command" => "executing",
                    _ => "working",
                };

                let brief = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(fn_args) {
                    parsed.get("path")
                        .or_else(|| parsed.get("command"))
                        .or_else(|| parsed.get("pattern"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    String::new()
                };

                println!("  \x1b[2m∗ {} •\x1b[0m \x1b[36m{}\x1b[0m \x1b[2m{}\x1b[0m", verb, fn_name, brief);
                let _ = io::stdout().flush();

                let t_start = Instant::now();
                let final_args = if cfg.adapters {
                    adapter::repair_tool_args(fn_name, fn_args)
                } else {
                    fn_args.to_string()
                };
                let tool_res = tools::execute_tool(fn_name, &final_args, target_dir, &boundary, cfg.yolo).await;
                let elapsed = t_start.elapsed().as_secs_f64();

                match tool_res {
                    Ok(out) => {
                        println!("  \x1b[1;32m✓\x1b[0m \x1b[2mDone ({:.1}s)\x1b[0m\n", elapsed);
                        history.push(ChatMessage::tool_response(call_id, out));
                    }
                    Err(err) => {
                        println!("  \x1b[1;31m✗\x1b[0m \x1b[2mFailed ({:.1}s): {}\x1b[0m\n", elapsed, err);
                        history.push(ChatMessage::tool_response(call_id, format!("[tool error] {}", err)));
                    }
                }
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = crossterm::terminal::disable_raw_mode();
            print!("\x1b[?25h\r\n");
            let _ = io::stdout().flush();
            std::process::exit(0);
        }
    });

    let cli = Cli::parse();
    let current_dir = env::current_dir()?;

    if let Some(Commands::Handoff) = &cli.command {
        let base_cfg = dirs::home_dir()
            .map(|h| h.join(".config/rs-agent/HANDOFF.md"))
            .unwrap_or_else(|| PathBuf::from("HANDOFF.md"));

        if let Ok(content) = fs::read_to_string(&base_cfg) {
            println!("{}", content);
        } else {
            eprintln!("\x1b[1;31mError:\x1b[0m Could not locate {}", base_cfg.display());
        }
        return Ok(());
    }

    if let Some(Commands::Match { query }) = &cli.command {
        let q = query.join(" ");
        let router = ContextRouter::load();
        if let Some(cmd) = router.resolve(&q) {
            println!("{}", cmd);
            std::process::exit(0);
        }
        std::process::exit(127);
    }

    if let Some(Commands::New { name, intents }) = &cli.command {
        handle_new_project(name, intents.as_deref())?;
        return Ok(());
    }

    if cli.bridge {
        println!("[rs-agent] Starting Bridge Mode (JSON-RPC on stdin)...");
        return Ok(());
    }

    match cli.command {
        Some(Commands::Init { path }) => {
            let target_path = path.map(|p| expand_path(&p)).unwrap_or(current_dir);
            run_session(&target_path, true).await?;
        }
        Some(Commands::Match { .. }) | Some(Commands::New { .. }) | Some(Commands::Handoff) => unreachable!(),
        None => {
            run_session(&current_dir, false).await?;
        }
    }

    Ok(())
}
