#![allow(dead_code)]

mod adapter;
mod client;
mod config;
mod matcher;
mod profile;
mod security;
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
use spinner::InlineSpinner;
use stream::StreamRenderer;
use std::env;
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
        if !target_dir.join(".rs/config.json").exists() {
            c.is_agent = true;

            let dir_name = target_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("agent")
                .to_string();

            let selected_profile = if let Some(p) = Profile::load(&dir_name) {
                p.name
            } else {
                Profile::select_interactive(target_dir).unwrap_or(dir_name)
            };

            c.profile_name = selected_profile;

            if let Some(p) = Profile::load(&c.profile_name) {
                c.tier = p.tier;
                c.yolo = p.yolo;
                c.thinking_budget = p.thinking_budget;
            } else {
                c.yolo = true;
            }

            c.save(target_dir)?;
        }
        c
    } else {
        WorkspaceConfig::load_or_default(target_dir)
    };

    let second_line = if cfg.is_agent {
        cfg.profile_name.clone()
    } else {
        "chat".to_string()
    };

    // Determine active box style: Profile frontmatter > Global state.json > Default (1)
    let active_profile = if cfg.is_agent {
        Profile::load(&cfg.profile_name)
    } else {
        Profile::load("chat")
    };

    let mut current_box_style = active_profile
        .as_ref()
        .and_then(|p| p.box_style)
        .unwrap_or_else(state::get_box_style);

    // Render box with smooth 350ms wake-on-key fade animation
    let mut buffered_char = ui::draw_box(&provider.model, &second_line, current_box_style, true);

    let system_prompt = if cfg.is_agent {
        if let Some(prof) = &active_profile {
            format!("{}\n\nWorkspace Path: {}", prof.system_prompt, target_dir.display())
        } else {
            format!("You are Rs, an autonomous agent working in workspace: {}.", target_dir.display())
        }
    } else {
        if let Some(prof) = &active_profile {
            prof.system_prompt.clone()
        } else {
            "You are an articulate, perceptive, and intellectually honest conversational partner.".to_string()
        }
    };

    let mut history = vec![ChatMessage::system(system_prompt)];
    let active_tools = if cfg.is_agent {
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

            print!("\r\x1b[1A\x1b[2K");
            ui::draw_box(&provider.model, &second_line, current_box_style, false);
            continue;
        }

        if input == "/clear" || input == "/c" {
            history.truncate(1);
            print!("\x1b[H\x1b[2J");
            ui::draw_box(&provider.model, &second_line, current_box_style, false);
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
                let tool_res = tools::execute_tool(fn_name, fn_args, target_dir, &boundary, cfg.yolo);
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
    // Graceful Ctrl+C: restores terminal cursor and exits 0 (~ ❯ instead of ~ ✗)
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

    if let Some(Commands::Match { query }) = &cli.command {
        let q = query.join(" ");
        let router = ContextRouter::load();
        if let Some(cmd) = router.resolve(&q) {
            println!("{}", cmd);
            std::process::exit(0);
        }
        std::process::exit(127);
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
        Some(Commands::Match { .. }) => unreachable!(),
        None => {
            run_session(&current_dir, false).await?;
        }
    }

    Ok(())
}
