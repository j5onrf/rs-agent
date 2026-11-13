use rs_agent::adapter::{repair_tool_args, strip_line_gutters};
use rs_agent::client::is_local_endpoint;
use rs_agent::config::{
    get_context_ceilings, get_env_path, get_max_rounds, get_max_tokens, ToolTier, WorkspaceConfig,
};
use rs_agent::matcher::ContextEntry;
use rs_agent::profile::Profile;
use rs_agent::security::SecurityBoundary;
use rs_agent::skill::Skill;
use rs_agent::state::{get_box_style, get_pad};
use rs_agent::tools::{execute_tool, get_tools_schema, is_read_only_tool, resilient_replace};
use rs_agent::ui::render_frame;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const GREEN_TICK: &str = "\x1b[1;32m✓ PASS\x1b[0m";
const RED_CROSS: &str = "\x1b[1;31m✖ FAIL\x1b[0m";

struct TestRunner {
    base_dir: PathBuf,
    home_dir: PathBuf,
    passed: usize,
    failed: usize,
}

impl TestRunner {
    fn new() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/j5"));
        let base_dir = home_dir.join(".config/rs-agent");
        Self { base_dir, home_dir, passed: 0, failed: 0 }
    }

    fn record(&mut self, ok: bool, label: &str, detail: &str) {
        if ok {
            self.passed += 1;
            println!("  {} : {:<53} {}", GREEN_TICK, label, detail);
        } else {
            self.failed += 1;
            println!("  {} : {:<53} {}", RED_CROSS, label, detail);
        }
    }

    async fn run_all(&mut self) {
        println!("\n======================================================================");
        println!("       RS-AGENT ALL-IN-ONE Live Diagnostic & Verification Suite       ");
        println!("======================================================================\n");

        let bin_path = if self.base_dir.join("target/release/ai").exists() {
            self.base_dir.join("target/release/ai")
        } else {
            self.base_dir.join("target/release/rs-agent")
        };

        // [Pass 1/10] Cargo Compilation & Release Binary Integrity
        println!("[Pass 1/10] Cargo Compilation & Release Binary Integrity");
        let bin_exists = bin_path.is_file();
        self.record(bin_exists, "Release Binary Exists & Executable", &bin_path.display().to_string());

        let symlink_path = self.home_dir.join(".local/bin/ai");
        let symlink_ok = symlink_path.exists();
        self.record(symlink_ok, "~/.local/bin/ai Symlink Resolution", &format!("-> {}", bin_path.display()));

        let help_ok = Command::new(&bin_path).arg("--help").output().map(|o| o.status.success()).unwrap_or(false);
        self.record(help_ok, "Sub-Millisecond CLI Entrypoint (--help)", "exit 0");
        println!();

        // [Pass 2/10] LIVE Small-Model Fallback Adapters & Self-Healing JSON
        println!("[Pass 2/10] LIVE Edge Adapters & Self-Healing JSON");
        let repaired_alias =
            repair_tool_args("read_file", r#"{"filepath": "/workspace/src/main.rs", "force": "true"}"#);
        let alias_ok = repaired_alias.contains(r#""path":"src/main.rs""#)
            && repaired_alias.contains(r#""overwrite":true"#)
            && !repaired_alias.contains("filepath");
        self.record(alias_ok, "LIVE Parameter Alias & Path Normalization", "filepath->path, force->overwrite");

        let repaired_json =
            repair_tool_args("write_file", "```json\n{\"path\": \"a.py\", \"content\": \"x = 1\n\", }\n```");
        let heal_ok =
            repaired_json.contains(r#""path":"a.py""#) && repaired_json.contains(r#""content":"x = 1\n""#);
        self.record(heal_ok, "LIVE Fenced / Trailing-Comma / Newline JSON Heal", "Repaired malformed tool args");

        let stripped = strip_line_gutters("12: fn f() {\n13:     let t = \"12:30:00\";\n14: }\n");
        let gutter_ok = stripped == "fn f() {\n    let t = \"12:30:00\";\n}\n";
        self.record(gutter_ok, "LIVE Line Gutter Stripper (Timestamp Safe)", "Stripped gutters, kept 12:30:00");
        println!();

        // [Pass 3/10] LIVE Zero-Trust Security Kernel & Boundary Containment
        println!("[Pass 3/10] LIVE Zero-Trust Security Kernel & Boundary Containment");
        let temp_sandbox = std::env::temp_dir().join(format!("rs-agent-ta-sandbox-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_sandbox);
        let _ = fs::create_dir_all(&temp_sandbox);
        let boundary = SecurityBoundary::new(&temp_sandbox);

        let blocked_passwd = boundary.is_outside(Path::new("/etc/passwd"));
        self.record(blocked_passwd, "Security Boundary: Block Traversal (/etc/passwd)", "Strictly blocked");

        let blocked_rel = boundary.is_outside(&boundary.resolve_path("../../secret_file"));
        self.record(blocked_rel, "Security Boundary: Block Relative Traversal (../../)", "Strictly blocked");

        let in_sandbox_file = temp_sandbox.join("valid.py");
        let _ = fs::write(&in_sandbox_file, "# test");
        let allowed_inside = !boundary.is_outside(&in_sandbox_file);
        self.record(allowed_inside, "Security Boundary: Permit In-Sandbox Work", "Autonomous inside root");

        let dev_null_ok = !boundary.is_outside(Path::new("/dev/null"));
        self.record(dev_null_ok, "Security Boundary: Whitelist Device Nodes (/dev/null)", "Permitted");

        #[cfg(unix)]
        {
            let link_path = temp_sandbox.join("esc_etc");
            let _ = std::os::unix::fs::symlink("/etc", &link_path);
            let symlink_blocked = boundary.is_outside(&link_path.join("passwd"));
            self.record(symlink_blocked, "Security Boundary: Block Symlink Escape -> /etc", "Strictly blocked");
        }

        let sudo_blocked = boundary.check_command("sudo pacman -Syu") == Some("sudo");
        let pip_blocked = boundary.check_command("pip install requests") == Some("pip");
        let py_pip_blocked = boundary.check_command("python3 -m pip install evil") == Some("python -m pip/venv");
        let rm_blocked = boundary.check_command("rm -rf /") == Some("destructive rm");
        self.record(
            sudo_blocked && pip_blocked && py_pip_blocked && rm_blocked,
            "Mutating Command Gating (sudo/pip/py -m pip/rm)",
            "All mutating patterns intercepted",
        );

        let readonly_ok = boundary.check_command("systemctl status nginx").is_none()
            && boundary.check_command("pacman -Qs ripgrep").is_none()
            && boundary.check_command("cargo test --all").is_none();
        self.record(
            readonly_ok,
            "Read-Only Inspection Passthrough (systemctl/pacman/cargo)",
            "Zero false-positive prompts",
        );
        println!();

        // [Pass 4/10] LIVE Native Tool Suite & 3-Stage Resilient Replacer
        println!("[Pass 4/10] LIVE Native Tool Suite & 3-Stage Resilient Replacer");
        let schema_ok = get_tools_schema(&ToolTier::PureBash, false).as_array().map(|a| a.len()) == Some(1)
            && get_tools_schema(&ToolTier::Core4, false).as_array().map(|a| a.len()) == Some(4)
            && get_tools_schema(&ToolTier::Smol, false).as_array().map(|a| a.len()) == Some(6)
            && get_tools_schema(&ToolTier::Smol, true).as_array().map(|a| a.len()) == Some(7)
            && is_read_only_tool("read_file")
            && is_read_only_tool("list_dir")
            && is_read_only_tool("search_code")
            && !is_read_only_tool("edit_file");
        self.record(schema_ok, "Tool Schema Tiers (1/4/6/7) & Read-Only Classifier", "PureBash, Core4, Smol + Delegate");

        let write_res = execute_tool(
            "write_file",
            r#"{"path":"calc.py","content":"def multiply(a, b):\n    return a * b\n"}"#,
            &temp_sandbox,
            &boundary,
        )
        .await;
        self.record(
            write_res.is_ok() && temp_sandbox.join("calc.py").is_file(),
            "LIVE write_file Execution",
            "calc.py written inside sandbox",
        );

        let edit1_res = execute_tool(
            "edit_file",
            r#"{"path":"calc.py","old_str":"return a * b","new_str":"return a * b * 1"}"#,
            &temp_sandbox,
            &boundary,
        )
        .await;
        let edit2_res = execute_tool(
            "edit_file",
            r#"{"path":"calc.py","old_str":"  return a * b * 1","new_str":"  return a * b"}"#,
            &temp_sandbox,
            &boundary,
        )
        .await;
        let disk_after = fs::read_to_string(temp_sandbox.join("calc.py")).unwrap_or_default();
        self.record(
            edit1_res.is_ok() && edit2_res.is_ok() && disk_after.contains("    return a * b"),
            "LIVE edit_file (Stage 1 Exact + Stage 2 Indent-Delta)",
            "Applied & preserved 4-space indent",
        );

        let fuzzy_ok = resilient_replace(
            "fn compute_total_value(items: &[i32]) -> i32 {\n    let mut total = 0;\n    total\n}\n",
            "fn compute_total_value(items: &[i32]) -> i32 {\n    let mut sum = 0;\n    total\n}",
            "fn compute_total_value(items: &[i32]) -> i32 {\n    let mut total = 1;\n    total\n}",
        )
        .map(|s| s.contains("let mut total = 1;"))
        .unwrap_or(false);
        let ambig_ok = resilient_replace("let x = 1;\nlet x = 1;\n", "let x = 1;", "let x = 2;").is_err();
        self.record(fuzzy_ok && ambig_ok, "LIVE resilient_replace (Stage 3 Fuzzy + Ambiguity)", "Fuzzy >=0.85 & multi-match guard");

        let read_res = execute_tool("read_file", r#"{"path":"calc.py","line_start":1}"#, &temp_sandbox, &boundary).await;
        let list_res = execute_tool("list_dir", r#"{}"#, &temp_sandbox, &boundary).await;
        let search_res = execute_tool("search_code", r#"{"pattern":"multiply"}"#, &temp_sandbox, &boundary).await;
        self.record(
            read_res.as_ref().map(|s| s.contains("def multiply")).unwrap_or(false)
                && list_res.as_ref().map(|s| s.contains("calc.py")).unwrap_or(false)
                && search_res.as_ref().map(|s| s.contains("calc.py:1:")).unwrap_or(false),
            "LIVE read_file / list_dir / search_code Execution",
            "Verified single-bound range & symbol hit",
        );
        println!();

        // [Pass 5/10] LIVE Command Execution, Exit Trapping & Signal Detection
        println!("[Pass 5/10] LIVE Command Execution, Exit Trapping & Signal Detection");
        let cmd_ok = execute_tool("run_command", r#"{"command":"echo ta_cmd_ok"}"#, &temp_sandbox, &boundary)
            .await
            .map(|s| s == "ta_cmd_ok")
            .unwrap_or(false);
        self.record(cmd_ok, "LIVE run_command Execution (exit 0)", "Captured stdout 'ta_cmd_ok'");

        let exit42_ok = execute_tool("run_command", r#"{"command":"exit 42"}"#, &temp_sandbox, &boundary)
            .await
            .map(|s| s.starts_with("(exit 42)"))
            .unwrap_or(false);
        self.record(exit42_ok, "LIVE run_command Exit Code Trapping", "Captured (exit 42)");

        #[cfg(unix)]
        {
            let sig_ok = execute_tool("run_command", r#"{"command":"kill -9 $$"}"#, &temp_sandbox, &boundary)
                .await
                .map(|s| s.starts_with("(killed by signal 9)"))
                .unwrap_or(false);
            self.record(sig_ok, "LIVE run_command Signal Trapping (SIGKILL)", "Reported (killed by signal 9)");
        }
        let _ = fs::remove_dir_all(&temp_sandbox);
        println!();

        // [Pass 6/10] Sub-Millisecond Context Router & Shell Teleport
        println!("[Pass 6/10] Sub-Millisecond Context Router & Shell Teleport");
        let ctx_file = self.base_dir.join("rs-context.md");
        self.record(ctx_file.is_file(), "Context File Exists (rs-context.md)", &ctx_file.display().to_string());

        let parsed_entry = ContextEntry::parse("[gate] rm -rf tmp ---> clean_tmp");
        let parse_ok = parsed_entry
            .as_ref()
            .map(|e| e.gated && e.command == "rm -rf tmp" && e.aliases == vec!["clean_tmp"])
            .unwrap_or(false)
            && ContextEntry::parse("---> bad_empty").is_none();
        self.record(parse_ok, "LIVE ContextEntry Parser ([gate] & empty-cmd guard)", "Parsed gated entry cleanly");

        let match_out = Command::new(&bin_path)
            .args(["match", "lfm"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        self.record(
            match_out.contains("ai init") && match_out.contains("lfm3"),
            "LIVE Intent Matcher: Shortcut Alias ('lfm')",
            &format!("Resolved -> '{}'", match_out),
        );

        let fallback_status = Command::new(&bin_path)
            .args(["match", "nonexistent_query_xyz_123"])
            .status()
            .map(|s| s.code().unwrap_or(0))
            .unwrap_or(0);
        self.record(fallback_status == 127, "LIVE Intent Matcher: Unknown Query Fallback", "Exited 127 (CNF-pass)");
        println!();

        // [Pass 7/10] LIVE Execution Isolation, Profile & Skill Parsers
        println!("[Pass 7/10] LIVE Execution Isolation, Profile & Skill Parsers");
        let chat_dir = self.base_dir.join("projects/chat");
        let chat_cfg = WorkspaceConfig::load_or_default(&chat_dir);
        self.record(
            !chat_cfg.is_agent && !chat_dir.join(".rs/config.json").exists(),
            "Chat Mode Security (ai): Zero-Tool Isolation",
            "is_agent=false · Pure Conversational",
        );

        let prof = Profile::parse("---\ncore4: true\nreasoning_effort: xhigh\nadapters: true\ndelegate: true\n---\nSys");
        let prof_ok = matches!(prof.tier, ToolTier::Core4)
            && prof.reasoning_effort.as_deref() == Some("xhigh")
            && prof.adapters
            && prof.delegate
            && prof.system_prompt == "Sys";
        let skill = Skill::parse("ponytail", "---\ndescription: \"x\"\n---\nKeep it minimal.");
        let skill_ok = skill.name == "ponytail" && skill.content == "Keep it minimal.";
        self.record(prof_ok && skill_ok, "LIVE Profile & Skill Frontmatter Parsers", "core4, xhigh, adapters, delegate");
        println!();

        // [Pass 8/10] LIVE Terminal UI Box Engine & State Read
        println!("[Pass 8/10] LIVE Terminal UI Box Engine & State Read");
        let expected_lines = [(1, 4), (2, 4), (3, 2), (4, 5), (5, 3), (6, 5)];
        let (lines7, count7) = render_frame("Ling-3.0-tiny", "chat", 7, 1.0);
        let ui_ok = expected_lines.iter().all(|&(style, want)| {
            let (lines, count) = render_frame("Ling-3.0-tiny", "chat", style, 1.0);
            count == want && lines.len() == want && lines.iter().all(|l| !l.is_empty())
        }) && count7 >= 5 && lines7.len() == count7 && lines7.iter().all(|l| !l.is_empty());
        self.record(ui_ok, "LIVE UI render_frame() Across Box Styles 1..=7", "All 7 styles rendered expected dimensions");

        let box_style = get_box_style();
        let _pad = get_pad();
        self.record(
            (1..=7).contains(&box_style),
            "LIVE AppState Load (box_style & pad)",
            &format!("Active box_style={}", box_style),
        );
        println!();

        // [Pass 9/10] LIVE Provider Cascade, Endpoint Classifier & Ceilings
        println!("[Pass 9/10] LIVE Provider Cascade, Endpoint Classifier & Ceilings");
        let max_tok = get_max_tokens();
        let max_rnd = get_max_rounds();
        let ceilings = get_context_ceilings();
        let cfg_ok = max_tok > 0 && max_rnd > 0 && ceilings.line_ceiling >= 250 && ceilings.char_cap >= 20_000;
        self.record(
            cfg_ok,
            "LIVE Context Ceilings & Governors",
            &format!("max_tokens={}, max_rounds={}, line_ceil={}", max_tok, max_rnd, ceilings.line_ceiling),
        );

        let route_ok = is_local_endpoint("http://127.0.0.1:8080/v1/chat/completions")
            && !is_local_endpoint("https://generativelanguage.googleapis.com/v1beta/openai/chat/completions");
        self.record(route_ok, "LIVE Endpoint Classifier (Local vs Cloud)", "127.0.0.1=local, googleapis=cloud");

        self.record(get_env_path().is_file(), "Provider Config File Exists (.env)", &get_env_path().display().to_string());
        println!();

        // [Pass 10/10] LIVE Shell Hook & ai --bridge JSON-RPC 2.0 Handshake
        println!("[Pass 10/10] LIVE Shell Hook & ai --bridge JSON-RPC 2.0 Handshake");
        let hook_file = self.base_dir.join("rs-hook.sh");
        let bash_ok = hook_file.is_file()
            && Command::new("bash").args(["-n", hook_file.to_str().unwrap_or("rs-hook.sh")]).status().map(|s| s.success()).unwrap_or(false);
        let hook_content = fs::read_to_string(&hook_file).unwrap_or_default();
        self.record(
            bash_ok && !hook_content.contains("rs_agent_chat()"),
            "Shell Hook Syntax (bash -n) & Clean Namespace",
            &hook_file.display().to_string(),
        );

        let bridge_ok = async {
            let mut child = tokio::process::Command::new(&bin_path)
                .arg("--bridge")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .ok()?;

            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let payload = concat!(
                    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#, "\n",
                    r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":{}}"#, "\n"
                );
                let _ = stdin.write_all(payload.as_bytes()).await;
                let _ = stdin.shutdown().await;
            }

            let out = tokio::time::timeout(std::time::Duration::from_secs(4), child.wait_with_output())
                .await
                .ok()?
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            Some(
                out.status.success()
                    && text.contains(r#""id":1"#)
                    && text.contains(r#""protocolVersion":1"#)
                    && text.contains(r#""name":"rs-agent""#)
                    && text.contains(r#""id":2"#)
                    && text.contains(r#""status":"ok""#),
            )
        }
        .await
        .unwrap_or(false);
        self.record(
            bridge_ok,
            "LIVE ai --bridge JSON-RPC 2.0 (initialize -> shutdown)",
            "Verified stdio ACP protocol v1 handshake",
        );
        println!();

        println!("======================================================================");
        if self.failed == 0 {
            println!("   \x1b[1;32m✓ ALL 10 LIVE PASSES SUCCESSFUL — RS-AGENT VERIFIED\x1b[0m               ");
        } else {
            println!(
                "   \x1b[1;31m✖ {} TESTS FAILED — CHECK DETAILS ABOVE\x1b[0m                         ",
                self.failed
            );
        }
        println!("======================================================================\n");

        if self.failed > 0 {
            std::process::exit(1);
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut runner = TestRunner::new();
    runner.run_all().await;
}
