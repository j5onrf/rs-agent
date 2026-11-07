use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const FORBIDDEN_SYS_DIRS: &[&str] =
    &["/etc", "/usr", "/var", "/bin", "/sbin", "/opt", "/root", "/boot", "/sys", "/proc"];

const SYSTEM_DEVICES: &[&str] = &["/dev/tty", "/dev/null", "/dev/urandom", "/dev/zero", "/dev/random"];

const MUTATING_COMMANDS: &[&str] = &[
    "sudo",
    "doas",
    "su",
    "pkexec",
    "pip",
    "pip3",
    "pipx",
    "pacman",
    "yay",
    "paru",
    "apt",
    "apt-get",
    "dnf",
    "yum",
    "systemctl",
    "reboot",
    "shutdown",
    "poweroff",
    "useradd",
    "usermod",
    "userdel",
    "passwd",
    "chroot",
    "mkfs",
    "dd",
    "fdisk",
    "parted",
    "wipefs",
];

static MUTATING_RE: OnceLock<regex::Regex> = OnceLock::new();
static PYTHON_PIP_RE: OnceLock<regex::Regex> = OnceLock::new();

fn get_mutating_re() -> &'static regex::Regex {
    MUTATING_RE.get_or_init(|| {
        let pattern = format!(r"\b({})\b", MUTATING_COMMANDS.join("|"));
        regex::Regex::new(&pattern).expect("Valid mutating commands regex")
    })
}

fn get_python_pip_re() -> &'static regex::Regex {
    PYTHON_PIP_RE.get_or_init(|| {
        regex::Regex::new(r"\bpython[0-9.]*\s+-m\s+(pip|pip3|pipx|ensurepip|venv)\b").expect("Valid python pip regex")
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

    /// Walk up to closest existing ancestor to canonicalize.
    /// Fails closed if target escapes the workspace root boundary.
    pub fn is_outside(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for &dev in SYSTEM_DEVICES {
            if path_str == dev {
                return false;
            }
        }

        let mut curr = path.to_path_buf();
        while !curr.exists() {
            if let Some(parent) = curr.parent() {
                if parent == curr {
                    break;
                }
                curr = parent.to_path_buf();
            } else {
                break;
            }
        }

        if let Ok(canon) = curr.canonicalize() {
            let canon_str = canon.to_string_lossy();
            for &dev in SYSTEM_DEVICES {
                if canon_str == dev {
                    return false;
                }
            }
            !canon.starts_with(&self.root)
        } else {
            true
        }
    }

    pub fn check_command(&self, cmd: &str) -> Option<&'static str> {
        let lower = cmd.to_lowercase();
        let _trimmed = lower.trim();

        // 1. Intercept shell commands targeting protected system trees outside workspace
        for &sys_dir in FORBIDDEN_SYS_DIRS {
            if lower.contains(sys_dir) {
                // Ignore benign grep/cat if inside dev
                return Some("system directory access attempt");
            }
        }
        let lower = cmd.to_lowercase();
        let trimmed = lower.trim();

        // 1. Python -m pip / venv module execution bypass
        if get_python_pip_re().is_match(trimmed) {
            return Some("python -m pip/venv");
        }

        // 2. Read-only inspection exemptions for systemctl and pacman
        if trimmed.starts_with("systemctl ") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let sub = parts[1];
                let readonly = matches!(
                    sub,
                    "status"
                        | "is-active"
                        | "is-enabled"
                        | "is-failed"
                        | "list-units"
                        | "list-unit-files"
                        | "list-timers"
                        | "list-sockets"
                        | "show"
                        | "cat"
                );
                if readonly {
                    return None;
                }
            }
        }

        if trimmed.starts_with("pacman ") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let flag = parts[1];
                let readonly = flag.starts_with("-q") || flag == "--query" || flag == "-ss" || flag == "-si";
                if readonly {
                    return None;
                }
            }
        }

        // 3. Mutating binary checks
        let re = get_mutating_re();
        if let Some(mat) = re.find(&lower) {
            let matched = mat.as_str();
            for &mutating in MUTATING_COMMANDS {
                if mutating == matched {
                    return Some(mutating);
                }
            }
        }

        // 4. Destructive shell patterns
        if lower.contains("rm -rf /") || lower.contains("rm -rf ~") || lower.contains("rm -rf *") {
            return Some("destructive rm");
        }
        if lower.contains("mkfs") || lower.contains("wipefs") || lower.contains("dd if=") {
            return Some("raw disk mutation");
        }

        None
    }

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
