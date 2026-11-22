# Git Snapshot Backup (`git-backup`)

Fast workspace snapshot tool that commits the current state and moves `backup-snapshot` to it (`git add -A`, `git commit --allow-empty`, `git branch -f backup-snapshot`).

## Shortcuts (`rs-context.md`)

| Shortcut | Command | Purpose |
| :--- | :--- | :--- |
| `gb` / `gitbackup` | `~/.config/rs-agent/tools/git-backup/git-backup` | Save snapshot & update `backup-snapshot` |
| `db` / `diffbackup` | `git diff backup-snapshot` | Show changes since last snapshot |

## Inspect

```bash
git diff backup-snapshot              # Full diff since snapshot
git diff --stat backup-snapshot       # Changed file summary
git log --oneline -n 10               # Recent snapshot commits
```

## Restore

```bash
# Full rollback to latest snapshot (discards uncommitted edits)
git reset --hard backup-snapshot

# Single-file rollback
git checkout backup-snapshot -- path/to/file.ext

# Roll back to an older snapshot from `git log --oneline`
git reset --hard <commit-hash>
```

## Squash Old Snapshots (Before Pushing)

Check `git log --oneline -n 15` first so you only squash local `State before changes` commits:

```bash
git reset --soft HEAD~10
git commit -m "Your clean commit message"
```

## Lazygit

Panel `3` (Local branches) -> highlight `backup-snapshot`: `d` to diff, `b` or `space` -> **Hard reset** to restore.
