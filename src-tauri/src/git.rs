//! Git for the review panel and per-thread worktrees. Shells out to `git`,
//! so behaviour (hooks, config, credentials) matches the user's terminal.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

async fn git(cwd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        // Never block on an editor or a credential prompt.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .output()
        .await
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if err.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { err };
        Err(if msg.is_empty() { format!("git {} failed", args.join(" ")) } else { msg })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct FileChange {
    pub path: String,
    /// M modified, A added, D deleted, R renamed, ? untracked, U conflicted.
    pub status: String,
    pub orig: Option<String>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Changes {
    pub repo: bool,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u64,
    pub behind: u64,
    pub files: Vec<FileChange>,
}

/// Collapses git's two-letter XY status (index, worktree) into one letter.
fn letter(xy: &str) -> String {
    let (x, y) = (xy.chars().next().unwrap_or('.'), xy.chars().nth(1).unwrap_or('.'));
    for c in ['D', 'A', 'R', 'C'] {
        if x == c || y == c {
            return if c == 'C' { "A".into() } else { c.to_string() };
        }
    }
    "M".into()
}

/// Parses `git status --porcelain=v2 --branch -z`.
pub fn parse_status(out: &str) -> Changes {
    let mut c = Changes { repo: true, ..Default::default() };
    let mut records = out.split('\0').filter(|r| !r.is_empty());
    while let Some(r) = records.next() {
        if let Some(h) = r.strip_prefix("# branch.head ") {
            c.branch = (h != "(detached)").then(|| h.to_string());
        } else if let Some(u) = r.strip_prefix("# branch.upstream ") {
            c.upstream = Some(u.to_string());
        } else if let Some(ab) = r.strip_prefix("# branch.ab ") {
            let mut it = ab.split_whitespace();
            c.ahead = it.next().and_then(|a| a.trim_start_matches('+').parse().ok()).unwrap_or(0);
            c.behind = it.next().and_then(|b| b.trim_start_matches('-').parse().ok()).unwrap_or(0);
        } else if let Some(p) = r.strip_prefix("? ") {
            c.files.push(FileChange { path: p.to_string(), status: "?".into(), ..Default::default() });
        } else if r.starts_with("1 ") {
            let f: Vec<&str> = r.splitn(9, ' ').collect();
            if let (Some(xy), Some(p)) = (f.get(1), f.get(8)) {
                c.files.push(FileChange { path: p.to_string(), status: letter(xy), ..Default::default() });
            }
        } else if r.starts_with("2 ") {
            // Renames: the original path is the next NUL-separated record.
            let f: Vec<&str> = r.splitn(10, ' ').collect();
            let orig = records.next().map(str::to_string);
            if let (Some(xy), Some(p)) = (f.get(1), f.get(9)) {
                c.files.push(FileChange { path: p.to_string(), status: letter(xy), orig, ..Default::default() });
            }
        } else if r.starts_with("u ") {
            let f: Vec<&str> = r.splitn(11, ' ').collect();
            if let Some(p) = f.get(10) {
                c.files.push(FileChange { path: p.to_string(), status: "U".into(), ..Default::default() });
            }
        }
    }
    c
}

/// `git diff --numstat -z HEAD` → path → (additions, deletions); binary files have none.
pub fn parse_numstat(out: &str) -> Vec<(String, Option<u64>, Option<u64>)> {
    let mut v = Vec::new();
    let mut records = out.split('\0').filter(|r| !r.is_empty());
    while let Some(r) = records.next() {
        let mut f = r.splitn(3, '\t');
        let (a, d, p) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
        // Renames put an empty path here, then old and new paths as records.
        let path = if p.is_empty() {
            let _old = records.next();
            records.next().unwrap_or("").to_string()
        } else {
            p.to_string()
        };
        v.push((path, a.parse().ok(), d.parse().ok()));
    }
    v
}

#[tauri::command]
pub async fn git_changes(cwd: String) -> Changes {
    let Ok(status) = git(&cwd, &["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"]).await else {
        return Changes::default();
    };
    let mut c = parse_status(&status);
    if let Ok(ns) = git(&cwd, &["diff", "--numstat", "-z", "HEAD"]).await {
        for (path, a, d) in parse_numstat(&ns) {
            if let Some(f) = c.files.iter_mut().find(|f| f.path == path) {
                f.additions = a;
                f.deletions = d;
            }
        }
    }
    for f in c.files.iter_mut().filter(|f| f.status == "?") {
        if let Ok(text) = tokio::fs::read_to_string(Path::new(&cwd).join(&f.path)).await {
            f.additions = Some(text.lines().count() as u64);
            f.deletions = Some(0);
        }
    }
    c
}

#[derive(Debug, Serialize)]
pub struct FileDiff {
    /// Content at HEAD; `None` for a new file.
    pub old: Option<String>,
    /// Content on disk; `None` for a deleted file.
    pub new: Option<String>,
    pub binary: bool,
    pub too_large: bool,
}

const MAX_DIFF_BYTES: usize = 1 << 20;

fn text_or_flag(bytes: Vec<u8>) -> (Option<String>, bool, bool) {
    if bytes.len() > MAX_DIFF_BYTES {
        return (None, false, true);
    }
    if bytes.contains(&0) {
        return (None, true, false);
    }
    (Some(String::from_utf8_lossy(&bytes).into_owned()), false, false)
}

#[tauri::command]
pub async fn git_file_diff(cwd: String, path: String, orig: Option<String>) -> Result<FileDiff, String> {
    let head_path = orig.unwrap_or_else(|| path.clone());
    let old_bytes = Command::new("git")
        .arg("-C")
        .arg(&cwd)
        .args(["show", &format!("HEAD:{head_path}")])
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .map(|o| o.stdout);
    let new_bytes = tokio::fs::read(Path::new(&cwd).join(&path)).await.ok();
    let (old, ob, ol) = old_bytes.map(text_or_flag).unwrap_or((None, false, false));
    let (new, nb, nl) = new_bytes.map(text_or_flag).unwrap_or((None, false, false));
    Ok(FileDiff { old, new, binary: ob || nb, too_large: ol || nl })
}

/// Stages everything and commits. Returns the new commit's short hash.
#[tauri::command]
pub async fn git_commit(cwd: String, message: String) -> Result<String, String> {
    if message.trim().is_empty() {
        return Err("write a commit message first".into());
    }
    git(&cwd, &["add", "-A"]).await?;
    git(&cwd, &["commit", "-q", "-m", message.trim()]).await?;
    Ok(git(&cwd, &["rev-parse", "--short", "HEAD"]).await?.trim().to_string())
}

/// Pushes the current branch, setting its upstream on first push.
#[tauri::command]
pub async fn git_push(cwd: String) -> Result<String, String> {
    let has_upstream = git(&cwd, &["rev-parse", "--abbrev-ref", "@{upstream}"]).await.is_ok();
    let args: &[&str] = if has_upstream { &["push"] } else { &["push", "-u", "origin", "HEAD"] };
    let out = Command::new("git")
        .arg("-C")
        .arg(&cwd)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    // git push reports progress on stderr even on success.
    let text = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        Ok(text)
    } else {
        Err(text)
    }
}

// ---- worktrees ----

pub fn worktrees_root() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("smithy/worktrees")
}

/// Branch and folder names for a thread's worktree.
/// Uses the whole thread id: ids embed the creation time in milliseconds, so
/// they never repeat, where a short suffix could collide across sessions.
pub fn worktree_names(project: &str, thread_id: &str) -> (String, PathBuf) {
    let name = Path::new(project).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
    (format!("smithy/{thread_id}"), worktrees_root().join(name).join(thread_id))
}

/// Creates a worktree on a new branch from the project's current HEAD.
pub async fn worktree_add(project: &str, thread_id: &str) -> Result<(String, String), String> {
    git(project, &["rev-parse", "--verify", "HEAD"])
        .await
        .map_err(|_| "worktrees need a git repository with at least one commit".to_string())?;
    let (branch, path) = worktree_names(project, thread_id);
    tokio::fs::create_dir_all(path.parent().unwrap()).await.map_err(|e| e.to_string())?;
    let p = path.to_string_lossy().into_owned();
    git(project, &["worktree", "add", "-q", "-b", &branch, &p, "HEAD"]).await?;
    Ok((branch, p))
}

/// Removes a thread's worktree. Refuses if it has uncommitted changes unless
/// `force`. Deletes the branch only if it's fully merged, so work is never lost.
pub async fn worktree_remove(project: &str, path: &str, branch: &str, force: bool) -> Result<(), String> {
    if Path::new(path).exists() {
        let dirty = !git(path, &["status", "--porcelain"]).await.unwrap_or_default().trim().is_empty();
        if dirty && !force {
            return Err("uncommitted changes in the worktree".into());
        }
        let mut args = vec!["worktree", "remove", path];
        if force {
            args.push("--force");
        }
        git(project, &args).await?;
    } else {
        let _ = git(project, &["worktree", "prune"]).await;
    }
    let _ = git(project, &["branch", "-d", branch]).await;
    Ok(())
}

/// Merges a thread's branch into whatever the main checkout has checked out.
#[tauri::command]
pub async fn git_merge(project: String, branch: String) -> Result<String, String> {
    let target = git(&project, &["rev-parse", "--abbrev-ref", "HEAD"]).await?.trim().to_string();
    git(&project, &["merge", "--no-ff", "--no-edit", &branch]).await.map_err(|e| {
        format!("merging {branch} into {target} failed: {e}\nResolve it in the project folder (or run `git merge --abort` there).")
    })?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2() {
        let out = "# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
                   1 .M N... 100644 100644 100644 aaa bbb src/a b.rs\0\
                   1 A. N... 000000 100644 100644 000 ccc new.rs\0\
                   2 R. N... 100644 100644 100644 ddd ddd R100 moved.rs\0old.rs\0\
                   ? notes.txt\0";
        let c = parse_status(out);
        assert_eq!(c.branch.as_deref(), Some("main"));
        assert_eq!(c.upstream.as_deref(), Some("origin/main"));
        assert_eq!((c.ahead, c.behind), (2, 1));
        let got: Vec<(&str, &str)> = c.files.iter().map(|f| (f.path.as_str(), f.status.as_str())).collect();
        assert_eq!(got, vec![("src/a b.rs", "M"), ("new.rs", "A"), ("moved.rs", "R"), ("notes.txt", "?")]);
        assert_eq!(c.files[2].orig.as_deref(), Some("old.rs"));
    }

    #[test]
    fn parses_numstat_with_renames_and_binaries() {
        let out = "3\t1\tsrc/a.rs\0-\t-\timg.png\0".to_string() + "0\t0\t\0old.rs\0moved.rs\0";
        let v = parse_numstat(&out);
        assert_eq!(v[0], ("src/a.rs".into(), Some(3), Some(1)));
        assert_eq!(v[1], ("img.png".into(), None, None));
        assert_eq!(v[2].0, "moved.rs");
    }

    #[test]
    fn worktree_names_are_stable() {
        let (b, p) = worktree_names("/home/x/proj", "18d2f9a1c0001");
        assert_eq!(b, "smithy/18d2f9a1c0001");
        assert!(p.ends_with("proj/18d2f9a1c0001"));
    }

    /// Full round trip in a scratch repo: worktree, change, status, commit, merge, remove.
    #[tokio::test]
    async fn worktree_round_trip() {
        let root = std::env::temp_dir().join(format!("smithy-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let r = repo.to_string_lossy().into_owned();
        for args in [&["init", "-q", "-b", "main"][..], &["config", "user.email", "t@t"], &["config", "user.name", "t"]] {
            git(&r, args).await.unwrap();
        }
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&r, &["add", "-A"]).await.unwrap();
        git(&r, &["commit", "-qm", "init"]).await.unwrap();

        let branch = "smithy/test1".to_string();
        let wt = root.join("wt").to_string_lossy().into_owned();
        git(&r, &["worktree", "add", "-q", "-b", &branch, &wt, "HEAD"]).await.unwrap();
        std::fs::write(Path::new(&wt).join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(Path::new(&wt).join("b.txt"), "new\n").unwrap();

        let c = git_changes(wt.clone()).await;
        assert_eq!(c.branch.as_deref(), Some("smithy/test1"));
        let a = c.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!((a.status.as_str(), a.additions, a.deletions), ("M", Some(1), Some(0)));
        assert_eq!(c.files.iter().find(|f| f.path == "b.txt").unwrap().status, "?");

        let d = git_file_diff(wt.clone(), "a.txt".into(), None).await.unwrap();
        assert_eq!(d.old.as_deref(), Some("one\n"));
        assert_eq!(d.new.as_deref(), Some("one\ntwo\n"));

        assert_eq!(worktree_remove(&r, &wt, &branch, false).await.unwrap_err(), "uncommitted changes in the worktree");
        git_commit(wt.clone(), "work".into()).await.unwrap();
        assert!(git_changes(wt.clone()).await.files.is_empty());
        assert_eq!(git_merge(r.clone(), branch.clone()).await.unwrap(), "main");
        assert_eq!(std::fs::read_to_string(repo.join("b.txt")).unwrap(), "new\n");
        worktree_remove(&r, &wt, &branch, false).await.unwrap();
        assert!(!Path::new(&wt).exists());
        assert!(git(&r, &["rev-parse", "--verify", &branch]).await.is_err(), "merged branch deleted");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
