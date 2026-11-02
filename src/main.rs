use clap::{Parser, Subcommand};
use rs_agent::adapter;
use rs_agent::client::{ChatMessage, LlmClient};
use rs_agent::config::{self, ProviderConfig, WorkspaceConfig};
use rs_agent::matcher::{self, ContextRouter};
use rs_agent::profile::Profile;
use rs_agent::security::SecurityBoundary;
use rs_agent::skill::{Skill, SkillAction};
use rs_agent::spinner::InlineSpinner;
use rs_agent::state;
use rs_agent::stream::StreamRenderer;
use rs_agent::tools;
use rs_agent::ui;
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

fn get_max_rounds() -> usize {
    fs::read_to_string(dirs::home_dir().map(|h| h.join(".config/rs-agent/.env")).unwrap_or_default())
        .ok()
        .and_then(|c| {
            for l in c.lines() {
                let s = l.trim();
                if !s.starts_with('#') && s.contains("AI_MAX_AGENT_ROUNDS=") {
                    let parts: Vec<&str> = s.splitn(2, '=').collect();
                    let v = parts.get(1)?.split('#').next()?.trim().trim_matches('"').trim_matches('\'');
                    return v.parse::<usize>().ok();
                }
            }
            None
        })
        .unwrap_or(15)
}

fn read_task_spec(target_dir: &Path) -> Option<String> {
    for fname in &["TASK.md", "task.md", "TODO.md"] {
        let p = target_dir.join(fname);
        if p.is_file() {
            if let Ok(c) = fs::read_to_string(&p) {
                let t = c.trim().to_string();
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
    }
    None
}

fn is_task_complete(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("task complete")
        || lower.contains("task completed")
        || lower.contains("goal complete")
        || lower.contains("goal completed")
        || lower.contains("all tests pass")
        || lower.contains("✓ task complete")
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
            "---\ndescription: \"{} Workspace Profile\"\n---\nROLE: Autonomous Systems & Software Engineer for {}.\n\nDIRECTIVES:\n- Inspect -> Edit -> Verify.\n- Stop immediately when tests pass with '✓ Task complete: <summary>'.\n",
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

    if is_agent {
        if let Some(prof) = active_profile {
            if prof.memory {
                let mem_path = target_dir.join(".rs/memory.md");
                if mem_path.is_file() {
                    if let Ok(mem_content) = fs::read_to_string(&mem_path) {
                        let trimmed = mem_content.trim();
                        if !trimmed.is_empty() {
                            prompt.push_str(&format!("\n\n---\nWORKSPACE MEMORY (.rs/memory.md):\n{}", trimmed));
                        }
                    }
                }
            }
        }
    }

    for sk_name in active_skills {
        if let Some(sk) = Skill::load(sk_name) {
            prompt.push_str(&format!("\n\n---\nSKILL DIRECTIVE [{}]:\n{}", sk.name, sk.content));
        }
    }

    prompt
}

async fn execute_task_loop(
    target_dir: &Path,
    goal: &str,
    client: &LlmClient,
    cfg: &WorkspaceConfig,
    boundary: &SecurityBoundary,
    history: &mut Vec<ChatMessage>,
    active_tools: Option<serde_json::Value>,
    spinner: &mut InlineSpinner,
) -> anyhow::Result<bool> {
    let max_rounds = get_max_rounds();
    let display_goal = if goal.len() > 100 { format!("{}...", &goal[..100]) } else { goal.to_string() };

    println!("  \x1b[1;36m✦ [Task Loop]\x1b[0m Starting autonomous run in \x1b[1;37m{}\x1b[0m", target_dir.display());
    println!("  \x1b[2mGoal: {} (Max steps: {})\x1b[0m\n", display_goal, max_rounds);

    history.push(ChatMessage::user(format!(
        "### Target Goal:\n{}\n\nExecute all necessary tool actions to complete and verify this goal. When 100% finished and verified, output '✓ Task complete: <summary>'.",
        goal
    )));

    let mut total_tools_run = 0;
    let mut consecutive_failures = 0;

    for turn in 0..max_rounds {
        println!("  \x1b[1;34m─── Step {}/{} ───\x1b[0m", turn + 1, max_rounds);

        if consecutive_failures >= 2 {
            history.push(ChatMessage::system(
                "[Harness Directive - Failure Decomposition] Your previous tool action failed repeatedly. Stop retrying the whole file. 1. Inspect exact lines with read_file or search_code. 2. Apply targeted edit_file to only that section. 3. Run verification with run_command."
            ));
            consecutive_failures = 0;
        }

        let mut renderer = StreamRenderer::new();
        spinner.start("Executing autonomous step...");

        let (reply, tool_calls, stats) = client.stream_chat(
            history,
            active_tools.clone(),
            &mut renderer,
            spinner,
        ).await?;

        spinner.stop();
        renderer.finish(&stats, 0);

        let mut turn_tool_failures = 0;

        if let Some(calls) = tool_calls {
            if !calls.is_empty() {
                let mut assistant_msg = ChatMessage::assistant(reply.clone());
                assistant_msg.tool_calls = Some(serde_json::Value::Array(calls.clone()));
                history.push(assistant_msg);

                for call in calls {
                    total_tools_run += 1;
                    let call_id = call["id"].as_str().unwrap_or("call_0").to_string();
                    let fn_name = call["function"]["name"].as_str().unwrap_or("tool");
                    let fn_args = call["function"]["arguments"].as_str().unwrap_or("{}");

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

                    println!("  \x1b[2m∗ executing •\x1b[0m \x1b[36m{}\x1b[0m \x1b[2m{}\x1b[0m", fn_name, brief);
                    let _ = io::stdout().flush();

                    let t_start = Instant::now();
                    let final_args = if cfg.adapters {
                        adapter::repair_tool_args(fn_name, fn_args)
                    } else {
                        fn_args.to_string()
                    };
                    let tool_res = tools::execute_tool(fn_name, &final_args, target_dir, boundary).await;
                    let elapsed = t_start.elapsed().as_secs_f64();

                    match tool_res {
                        Ok(out) => {
                            println!("  \x1b[1;32m✓\x1b[0m \x1b[2mDone ({:.1}s)\x1b[0m\n", elapsed);
                            history.push(ChatMessage::tool_response(call_id, out));
                        }
                        Err(err) => {
                            turn_tool_failures += 1;
                            println!("  \x1b[1;31m✗\x1b[0m \x1b[2mFailed ({:.1}s): {}\x1b[0m\n", elapsed, err);
                            history.push(ChatMessage::tool_response(call_id, format!("[tool error] {}", err)));
                        }
                    }
                }
            } else {
                history.push(ChatMessage::assistant(reply.clone()));
            }
        } else {
            history.push(ChatMessage::assistant(reply.clone()));
        }

        if turn_tool_failures > 0 {
            consecutive_failures += 1;
        } else {
            consecutive_failures = 0;
        }

        if is_task_complete(&reply) {
            let lower_goal = goal.to_lowercase();
            let requires_action = lower_goal.contains("create")
                || lower_goal.contains("write")
                || lower_goal.contains("edit")
                || lower_goal.contains("test")
                || lower_goal.contains("fix")
                || lower_goal.contains("add");

            if total_tools_run == 0 && requires_action {
                println!("  \x1b[1;33m▲ [Task Loop]\x1b[0m Completion declared without tool execution. Enforcing verification...");
                history.push(ChatMessage::user(
                    "You declared completion without running tools. Execute the required write_file or run_command now."
                ));
                continue;
            }

            println!("\n  \x1b[1;32m✓ [Task Loop] Goal completed in {} step(s) ({} tool action(s) executed)!\x1b[0m\n", turn + 1, total_tools_run);
            return Ok(true);
        }

        history.push(ChatMessage::user(
            "Continue with the next step. If completely finished and verified, emit '✓ Task complete: <summary>'."
        ));
    }

    println!("\n  \x1b[1;33m▲ [Task Loop] Reached max limit ({}) without completion marker.\x1b[0m\n", max_rounds);
    Ok(false)
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
            c.adapters = p.adapters;
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

        if input == "/t3" || input.starts_with("/t3 ") {
            let sub = input.strip_prefix("/t3").unwrap().trim();
            let launcher = dirs::home_dir()
                .map(|h| h.join(".config/rs-agent/plugins/t3code/launch.sh"))
                .unwrap_or_else(|| PathBuf::from("plugins/t3code/launch.sh"));

            if launcher.is_file() {
                let mut cmd = std::process::Command::new("bash");
                cmd.arg(&launcher);
                if sub == "web" || sub == "--web" {
                    cmd.arg("web");
                }
                cmd.arg(target_dir);
                cmd.env("AI_ACTIVE_PROFILE", &cfg.profile_name);
                cmd.env("AI_WORKSPACE_PATH", target_dir);

                println!("  \x1b[1;36m[t3code]\x1b[0m Handing over workspace to T3 Code...");
                let _ = io::stdout().flush();

                match cmd.status() {
                    Ok(s) if s.success() => {
                        println!("  \x1b[1;32m✓\x1b[0m \x1b[2mT3 Code session active for workspace\x1b[0m \x1b[1;36m{}\x1b[0m\n", target_dir.display());
                    }
                    Ok(_) => {
                        println!("  \x1b[1;33m▲\x1b[0m \x1b[2mT3 Code window closed.\x1b[0m\n");
                    }
                    Err(e) => {
                        println!("  \x1b[1;31m✗\x1b[0m \x1b[2mFailed launching T3 Code: {}\x1b[0m\n", e);
                    }
                }
            } else {
                println!("  \x1b[1;31m✗\x1b[0m \x1b[2mPlugin launcher missing: {}\x1b[0m\n", launcher.display());
            }
            continue;
        }

        if input == "/task" || input == "/goal" || input.starts_with("/task ") || input.starts_with("/goal ") {
            let direct_arg = if input.starts_with("/task ") {
                input[6..].trim()
            } else if input.starts_with("/goal ") {
                input[6..].trim()
            } else {
                ""
            };

            let effective_goal = if !direct_arg.is_empty() {
                Some(direct_arg.to_string())
            } else {
                read_task_spec(target_dir)
            };

            if let Some(goal) = effective_goal {
                let _ = execute_task_loop(
                    target_dir,
                    &goal,
                    &client,
                    &cfg,
                    &boundary,
                    &mut history,
                    active_tools.clone(),
                    &mut spinner,
                ).await;
            } else {
                println!("  \x1b[1;33m[task]\x1b[0m No objective provided and no TASK.md found.");
                println!("         \x1b[2m• Usage:  /task <describe goal>\x1b[0m");
                println!("         \x1b[2m• Or:     Create TASK.md in project root with markdown checkboxes.\x1b[0m\n");
            }
            continue;
        }

        if input == "/p" || input == "/profile" {
            print!("\x1b[1A\r\x1b[2K");
            let _ = io::stdout().flush();

            if !cfg.is_agent {
                println!("  \x1b[1;33m[info]\x1b[0m /p is only available in workspace agents (ai init <path>).");
                println!("         Chat mode is locked to the pure conversation profile.\n");
                continue;
            }

            let has_conversation = history.iter().any(|m| m.role == "user");
            if has_conversation {
                print!("  \x1b[1;33m[warn]\x1b[0m Switching profile will erase active conversation. Proceed? [y/N] ");
                let _ = io::stdout().flush();
                let mut conf = String::new();
                if io::stdin().read_line(&mut conf).is_err() || !matches!(conf.trim().to_lowercase().as_str(), "y" | "yes") {
                    print!("\x1b[1A\r\x1b[2K");
                    let _ = io::stdout().flush();
                    println!("  \x1b[2mProfile switch cancelled.\x1b[0m\n");
                    continue;
                }
                print!("\x1b[1A\r\x1b[2K");
                let _ = io::stdout().flush();
            }

            if let Some(new_prof) = Profile::select_interactive(target_dir, &cfg.profile_name) {
                cfg.profile_name = new_prof;
                active_profile = Profile::load(&cfg.profile_name);

                if let Some(p) = &active_profile {
                    cfg.tier = p.tier.clone();
                    cfg.adapters = p.adapters;
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

                print!("\x1b[H\x1b[2J");
                println!();
                println!("\x1b[1;32m✓\x1b[0m \x1b[2mProfile set to:\x1b[0m \x1b[1;36m{}\x1b[0m", cfg.profile_name);
                ui::draw_box(&provider.model, &cfg.profile_name, current_box_style, false);
                let _ = io::stdout().flush();
            }
            continue;
        }

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

        if input.starts_with("/box") {
            let parts: Vec<&str> = input.split_whitespace().collect();
            current_box_style = if parts.len() > 1 {
                parts[1].parse::<usize>().unwrap_or((current_box_style % 3) + 1)
            } else {
                (current_box_style % 3) + 1
            };
            state::set_box_style(current_box_style);

            print!("\x1b[H\x1b[2J");
            println!();
            println!("\x1b[1;32m✓\x1b[0m \x1b[2mTheme set to box style\x1b[0m \x1b[1;36m{}\x1b[0m", current_box_style);
            let second_line = if cfg.is_agent { &cfg.profile_name } else { "chat" };
            ui::draw_box(&provider.model, second_line, current_box_style, false);
            let _ = io::stdout().flush();
            continue;
        }

        if input == "/r" || input == "/reset" {
            print!("\x1b[1A\r\x1b[2K");
            let _ = io::stdout().flush();

            history.clear();
            system_prompt = build_system_prompt(cfg.is_agent, target_dir, &active_profile, &active_skills);
            history.push(ChatMessage::system(&system_prompt));

            print!("\x1b[H\x1b[2J");
            println!();
            println!("\x1b[1;32m✓\x1b[0m \x1b[2mSession reset\x1b[0m");
            let second_line = if cfg.is_agent { &cfg.profile_name } else { "chat" };
            ui::draw_box(&provider.model, second_line, current_box_style, false);
            let _ = io::stdout().flush();
            continue;
        }

        if input == "/clear" || input == "/c" {
            print!("\x1b[H\x1b[2J");
            println!();
            println!("\x1b[1;32m✓\x1b[0m \x1b[2mSession reset\x1b[0m");
            let second_line = if cfg.is_agent { &cfg.profile_name } else { "chat" };
            ui::draw_box(&provider.model, second_line, current_box_style, false);
            let _ = io::stdout().flush();
            continue;
        }

        let router = matcher::ContextRouter::load();
        if let Some(entry) = router.resolve_entry(input) {
            let cmd = entry.command;

            let is_destructive = cmd.contains("rm ") || cmd.contains("sudo ") || cmd.contains("reset --hard");
            if entry.gated || is_destructive {
                print!("  \x1b[1;33m[gate]\x1b[0m Execute '\x1b[1;36m{}\x1b[0m'? [y/N] ", cmd);
                let _ = io::stdout().flush();
                let mut conf = String::new();
                if io::stdin().read_line(&mut conf).is_err() || !matches!(conf.trim().to_lowercase().as_str(), "y" | "yes") {
                    println!("  \x1b[2mCancelled.\x1b[0m\n");
                    continue;
                }
                println!();
            }
            if cmd.starts_with("ai init ") {
                println!("  \x1b[1;33m[info]\x1b[0m Workspace teleports ('{}') are used from your terminal shell.\n", input);
                continue;
            }

            let expanded_cmd = if let Ok(home) = std::env::var("HOME") {
                cmd.replace("~/", &format!("{}/", home))
            } else {
                cmd
            };

            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg(&expanded_cmd)
                .status();

            println!();
            continue;
        }

        println!();
        io::stdout().flush()?;

        history.push(ChatMessage::user(input));

        let max_rounds = get_max_rounds();
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
                let tool_res = tools::execute_tool(fn_name, &final_args, target_dir, &boundary).await;
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
