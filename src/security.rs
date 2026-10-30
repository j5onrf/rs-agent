use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

// Dangerous binaries that alter system state or packages
const MUTATING_COMMANDS: &[&str] = &[
    "sudo", "doas", "su", "pkexec",
    "pip", "pip3", "pipx",
    "pacman", "yay", "paru", "apt", "dnf",
    "systemctl", "reboot", "shutdown", "poweroff",
    "mkfs", "dd", "fdisk", "parted",
];

pub struct SecurityBoundary {
    pub root: PathBuf,
}

impl SecurityBoundary {
    pub fn new<P: AsRef<Path>>(workspace: P) -> Self {
        let root = workspace.as_ref().canonicalize().unwrap_or_else(|_| workspace.as_ref().to_path_buf());
        Self { root }
    }

    /// Resolves target path expanding ~ and relative components
    pub fn resolve_path(&self, target: &str) -> PathBuf {
        let clean = target.trim().trim_matches(|c| c == '\'' || c == '"' || c == '`');
        if let Some(stripped) = clean.strip_prefix("~/") {
            dirs::home_dir().map(|h| h.join(stripped)).unwrap_or_else(|| PathBuf::from(clean))
        } else if clean.starts_with('/') {
            PathBuf::from(clean)
        } else {
            self.root.join(clean)
        }
    }

    /// Returns true if the target path breaks outside the workspace root
    pub fn is_outside(&self, path: &Path) -> bool {
        if path.exists() {
            if let Ok(canon) = path.canonicalize() {
                return !canon.starts_with(&self.root);
            }
        } else if let Some(parent) = path.parent() {
            if let Ok(canon_parent) = parent.canonicalize() {
                return !canon_parent.starts_with(&self.root);
            }
        }
        true
    }

    /// Flags system mutating package managers or destructive shell calls
    pub fn check_command(&self, cmd: &str) -> Option<&'static str> {
        let lower = cmd.to_lowercase();
        for &mutating in MUTATING_COMMANDS {
            let pattern = format!(r"\b{}\b", mutating);
            if regex::Regex::new(&pattern).map(|r| r.is_match(&lower)).unwrap_or(false) {
                return Some(mutating);
            }
        }
        if lower.contains("rm -rf /") || lower.contains("rm -rf ~") || lower.contains("rm -rf *") {
            return Some("destructive rm");
        }
        None
    }

    /// Invariant gate: Never bypass security events even in YOLO; auto-approve safe in-bounds actions in YOLO
    pub fn authorize(&self, action: &str, is_security_event: bool, yolo: bool) -> bool {
        // Safe in-bounds action with YOLO active: 100% Autonomous (0 Prompts)
        if !is_security_event && yolo {
            return true;
        }

        let prompt_color = if is_security_event { "\x1b[1;31m" } else { "\x1b[1;33m" };
        let tag = if is_security_event { "[SECURITY-ALERT]" } else { "[gate]" };

        print!("\n{}{} Authorize: {}? [y/N]: \x1b[0m", prompt_color, tag, action);
        let _ = io::stdout().flush();

        // Read directly from /dev/tty so stdin remains clean
        if let Ok(tty) = fs::File::open("/dev/tty") {
            let mut reader = io::BufReader::new(tty);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok() {
                let ans = line.trim().to_lowercase();
                return ans == "y" || ans == "yes";
            }
        }
        false
    }
}
