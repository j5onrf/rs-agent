# Git Snapshot Backup (`git-backup`)

A minimalist, sub-10ms workspace snapshot tool for `rs-agent`. Captures the exact workspace state on the `backup-snapshot` branch before autonomous agent execution, experimental refactoring, or destructive tool runs.

---

## 1. Quick In-Agent Shortcuts (`rs-context.md`)

When working inside any workspace (`flint`, `groot`, `lfm3`), these shortcuts run directly from the `❯` prompt with safety gates:

| Shortcut | Command Executed | Purpose |
| :--- | :--- | :--- |
| `gb` / `gitbackup` | `~/.config/rs-agent/tools/git-backup/git-backup` | Creates/updates `backup-snapshot` branch (Gated) |
| `db` / `diffbackup` | `git diff backup-snapshot` | Displays color diff of all changes since snapshot |

---

## 2. Inspecting the Backup

### Check Changes Since Snapshot (Diff)
```bash
# View full unified diff of all changes made since the backup
git diff backup-snapshot

# View high-level file modification summary (+/- line counts)
git diff --stat backup-snapshot

# Check diff for a specific file only
git diff backup-snapshot -- src/main.rs
```

### Inspect Backup Details & Commit
```bash
# View latest snapshot commit hash, timestamp, and files committed
git log -1 backup-snapshot --stat

# View full commit message and patch of the snapshot
git show backup-snapshot
```

---

## 3. Restoration & Rollback

### Full Workspace Rollback (Hard Reset)
Instantly wipes all changes and uncommitted files made after the snapshot, restoring the repository to the exact second the backup was taken:
```bash
git reset --hard backup-snapshot
```

### Surgical Single-File Rollback
Restore a single file corrupted or incorrectly modified by an agent without touching any other changes in the workspace:
```bash
# Revert only one file back to the snapshot state
git checkout backup-snapshot -- path/to/file.ext

# Or using modern git restore:
git restore --source=backup-snapshot path/to/file.ext
```

### Soft Rollback (Review Mode)
Moves the branch pointer back to the snapshot, but leaves all modified files staged in your working directory for manual inspection:
```bash
git reset --soft backup-snapshot
```

---

## 4. Visual Inspection in Lazygit

If you use `lazygit`, the snapshot branch is accessible visually:

1. **View Diff**:
   - Press `3` to jump to **[3]-Local branches**.
   - Highlight `backup-snapshot`.
   - Press `d` to toggle a live split diff of all changes made against the snapshot.
2. **Rollback**:
   - In panel `3`, highlight `backup-snapshot`.
   - Press `b` or `space` to open the branch menu.
   - Select **Hard reset** to restore.

---

## 5. Technical Mechanics

The tool executes:
```bash
git add -A
git commit -m "State before changes: $(date +'%Y-%m-%d %H:%M:%S')" --allow-empty --quiet
git branch -f backup-snapshot
```

- **`--allow-empty`**: Guarantees the tool never fails or exits with an error code even if the working tree is completely clean.
- **`git branch -f`**: Atomically updates the `backup-snapshot` branch pointer to the current commit without creating duplicate branch clutter.
- **Zero residue**: No temporary files or staging caches are left behind.
