use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MUTATING_COMMANDS: &[&str] = &[
    "sudo", "doas", "su", "pkexec",
    "pip", "pip3", "pipx",
    "pacman", "yay", "paru", "apt", "dnf",
    "systemctl", "reboot", "shutdown", "poweroff",
    "mkfs", "dd", "fdisk", "parted",
];

static MUTATING_RE: OnceLock<regex::Regex> = OnceLock::new();

fn get_mutating_re() -> &'static regex::Regex {
    MUTATING_RE.get_or_init(|| {
        let pattern = format!(r"\b({})\b", MUTATING_COMMANDS.join("|"));
        regex::Regex::new(&pattern).expect("Valid mutating commands regex")
    })
}

pub struct SecurityBoundary {
    pub root: PathBuf,
}

impl SecurityBoundary {
    pub fn new<P: AsRef<Path>>(workspace: P) -> Self {
        let root = workspace.as_ref().canonicalize().unwrap_or_else(|_| workspace.as_ref().to_path_buf());
        Self { root }
    }

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

    pub fn check_command(&self, cmd: &str) -> Option<&'static str> {
        let lower = cmd.to_lowercase();
        let re = get_mutating_re();
        if let Some(mat) = re.find(&lower) {
            let matched = mat.as_str();
            for &mutating in MUTATING_COMMANDS {
                if mutating == matched {
                    return Some(mutating);
                }
            }
        }
        if lower.contains("rm -rf /") || lower.contains("rm -rf ~") || lower.contains("rm -rf *") {
            return Some("destructive rm");
        }
        None
    }

    /// Hard security gate: prompts only on boundary violations or mutating binaries
    pub fn authorize(&self, violation: &str) -> bool {
        print!("\n\x1b[1;31m[SECURITY-ALERT] Authorize: {}? [y/N]: \x1b[0m", violation);
        let _ = io::stdout().flush();

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
