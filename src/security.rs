use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
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
        regex::Regex::new(r"\bpython[0-9.]*\s+-m\s*(pip|pip3|pipx|ensurepip|venv)\b").expect("Valid python pip regex")
    })
}

#[inline]
fn is_shell_boundary(b: u8) -> bool {
    b.is_ascii_whitespace()
        || matches!(
            b,
            b'=' | b':' | b'"' | b'\'' | b'`' | b';' | b'(' | b')' | b'<' | b'>' | b'&' | b'|' | b'{' | b',' | b'*'
        )
}

fn contains_sys_dir_token(lower: &str, sys_dir: &str) -> bool {
    let bytes = lower.as_bytes();
    for (idx, _) in lower.match_indices(sys_dir) {
        let before_ok = idx == 0 || is_shell_boundary(bytes[idx - 1]);
        let after_idx = idx + sys_dir.len();
        let after_ok = after_idx == bytes.len() || bytes[after_idx] == b'/' || is_shell_boundary(bytes[after_idx]);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Detection of destructive rm commands across shell separators
fn is_destructive_rm(cmd: &str) -> bool {
    for segment in cmd.split([';', '|', '&', '\n', '(', ')']) {
        let mut iter = segment.split_whitespace();
        while let Some(t) = iter.next() {
            if t == "rm" {
                let mut has_r = false;
                let mut has_f = false;
                let mut has_dangerous_target = false;

                for arg in iter.by_ref() {
                    if arg == "--recursive" || arg == "-r" || arg == "-R" {
                        has_r = true;
                    } else if arg == "--force" || arg == "-f" || arg == "-F" {
                        has_f = true;
                    } else if arg.starts_with('-') && !arg.starts_with("--") {
                        let flags = &arg[1..];
                        if flags.contains('r') || flags.contains('R') {
                            has_r = true;
                        }
                        if flags.contains('f') || flags.contains('F') {
                            has_f = true;
                        }
                    } else if arg == "/"
                        || arg == "~"
                        || arg == "*"
                        || arg.starts_with("/etc")
                        || arg.starts_with("/home")
                        || arg.contains("..")
                    {
                        has_dangerous_target = true;
                    }
                }

                if has_dangerous_target || (has_r && has_f) {
                    return true;
                }
            }
        }
    }
    false
}

pub struct SecurityBoundary {
    pub root: PathBuf,
    pub headless: bool,
}

impl SecurityBoundary {
    pub fn new<P: AsRef<Path>>(workspace: P) -> Self {
        let root = workspace.as_ref().canonicalize().unwrap_or_else(|_| workspace.as_ref().to_path_buf());
        Self { root, headless: false }
    }

    pub fn new_headless<P: AsRef<Path>>(workspace: P) -> Self {
        let mut b = Self::new(workspace);
        b.headless = true;
        b
    }

    pub fn child<P: AsRef<Path>>(&self, workspace: P) -> Self {
        let mut b = Self::new(workspace);
        b.headless = self.headless;
        b
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
    /// Strictly inspects symlinks to ensure targets cannot escape workspace root.
    pub fn is_outside(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for &dev in SYSTEM_DEVICES {
            if path_str == dev {
                return false;
            }
        }

        let mut curr = path.to_path_buf();

        // Walk ancestors until an entry with metadata is found (symlink_metadata does NOT follow symlinks)
        while fs::symlink_metadata(&curr).is_err() {
            if let Some(parent) = curr.parent() {
                if parent == curr {
                    break;
                }
                curr = parent.to_path_buf();
            } else {
                break;
            }
        }

        // If curr is a symlink, verify its target directly to block dangling symlink escapes
        if let Ok(meta) = fs::symlink_metadata(&curr) {
            if meta.is_symlink() {
                if let Ok(target) = curr.canonicalize() {
                    if !target.starts_with(&self.root) {
                        return true; // Resolved symlink target escapes boundary
                    }
                } else if let Ok(link_target) = fs::read_link(&curr) {
                    let resolved = if link_target.is_absolute() {
                        link_target
                    } else {
                        curr.parent().unwrap_or(&self.root).join(link_target)
                    };
                    let mut target_ancestor = resolved;
                    while fs::symlink_metadata(&target_ancestor).is_err() {
                        if let Some(p) = target_ancestor.parent() {
                            if p == target_ancestor {
                                break;
                            }
                            target_ancestor = p.to_path_buf();
                        } else {
                            break;
                        }
                    }
                    if let Ok(canon) = target_ancestor.canonicalize() {
                        if !canon.starts_with(&self.root) {
                            return true; // Target ancestor escapes boundary
                        }
                    } else {
                        return true; // Fail closed
                    }
                } else {
                    return true; // Fail closed
                }
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
        let trimmed = lower.trim();

        // 1. Intercept shell commands targeting protected system trees outside workspace
        for &sys_dir in FORBIDDEN_SYS_DIRS {
            if contains_sys_dir_token(&lower, sys_dir) {
                return Some("system directory access attempt");
            }
        }

        // 2. Python -m pip / venv module execution bypass (catches -mpip and -m pip)
        if get_python_pip_re().is_match(trimmed) {
            return Some("python -m pip/venv");
        }

        // 3. Read-only inspection exemptions (ONLY permitted if NO shell chaining or redirection exists)
        let has_chaining = trimmed.contains([';', '|', '&', '`', '$', '\n', '>', '<']);

        if !has_chaining && trimmed.starts_with("systemctl ") {
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

        if !has_chaining && trimmed.starts_with("pacman ") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let flag = parts[1];
                let readonly = flag.starts_with("-q") || flag == "--query" || flag == "-ss" || flag == "-si";
                if readonly {
                    return None;
                }
            }
        }

        // 4. Mutating binary checks
        let re = get_mutating_re();
        if let Some(mat) = re.find(&lower) {
            let matched = mat.as_str();
            for &mutating in MUTATING_COMMANDS {
                if mutating == matched {
                    return Some(mutating);
                }
            }
        }

        // 5. Destructive rm detection (tokenized flag-cluster analysis)
        if is_destructive_rm(&lower) {
            return Some("destructive rm");
        }

        // 6. Raw disk mutation
        if lower.contains("mkfs") || lower.contains("wipefs") || lower.contains("dd if=") {
            return Some("raw disk mutation");
        }

        None
    }

    pub fn authorize(&self, violation: &str) -> bool {
        if self.headless || !io::stdout().is_terminal() {
            return false;
        }
        if io::stdin().is_terminal() {
            return crate::ui::authorize_prompt(violation);
        }
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_command_avoids_workspace_false_positives_and_catches_real_escapes() {
        let b = SecurityBoundary::new_headless(".");
        assert_eq!(b.check_command("./.venv/bin/pytest -q"), None);
        assert_eq!(b.check_command("cargo test --manifest-path src/sys/Cargo.toml"), None);
        assert_eq!(b.check_command("node_modules/.bin/tsc --noEmit"), None);
        assert!(b.check_command("cat /etc/passwd").is_some());
        assert!(b.check_command("PATH=/usr/bin:/bin ls").is_some());
        assert!(b.check_command("echo ok;rm -rf .").is_some());
        assert!(!b.authorize("test"));
    }
}
