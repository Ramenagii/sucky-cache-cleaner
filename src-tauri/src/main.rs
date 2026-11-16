#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Emitter;
use walkdir::WalkDir;

// Config lives in the same place the egui-era app used, so existing repos carry over.
const CONFIG_DIR: &str = "rush-button";
const CONFIG_FILE: &str = "config.json";

#[derive(Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    rush_repos: Vec<String>,
}

fn config_path() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|p| PathBuf::from(p).join(CONFIG_DIR).join(CONFIG_FILE))
}

fn load_config() -> Config {
    config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(cfg: &Config) {
    if let Some(p) = config_path() {
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(cfg) {
            let _ = fs::write(&p, json);
        }
    }
}

fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn emit_job(app: &tauri::AppHandle, id: &str, label: &str) {
    let _ = app.emit("job-progress", json!({ "id": id, "label": label }));
}

fn tool_on_path(tool: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    command(finder)
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn tool_output(tool: &str, args: &[&str], cwd: Option<&Path>) -> Option<String> {
    let mut cmd = command(tool);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.output().ok().and_then(|o| {
        if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else {
            None
        }
    })
}

fn scan_dir(path: &Path) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            files += 1;
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    (bytes, files)
}

fn purge_dir_contents(root: &Path, lines: &mut Vec<String>) -> (u64, u32) {
    let mut freed = 0u64;
    let mut errors = 0u32;
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(e) => {
            lines.push(format!("  ! cannot read {}: {e}", root.display()));
            return (0, 1);
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let size = if path.is_dir() {
            scan_dir(&path).0
        } else {
            fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
        };
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match result {
            Ok(()) => freed += size,
            Err(e) => {
                errors += 1;
                lines.push(format!("  ! skip {}: {e}", path.display()));
            }
        }
    }
    (freed, errors)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum TargetKind {
    Temp,
    Npm,
    Pip,
    Pnpm,
    Yarn,
    NuGet,
}

const BASE_TARGETS: &[(TargetKind, &str, bool)] = &[
    (TargetKind::Temp, "User TEMP folder", false),
    (TargetKind::Npm, "npm cache", true),
    (TargetKind::Pip, "pip cache", true),
    (TargetKind::Pnpm, "pnpm store", true),
    (TargetKind::Yarn, "yarn cache", true),
    (TargetKind::NuGet, "NuGet packages", false),
];

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TargetInfo {
    id: String,
    label: String,
    path: Option<String>,
    available: bool,
    note: String,
    checked: bool,
    size_bytes: Option<u64>,
    file_count: Option<u64>,
}

fn resolve_base_target(kind: TargetKind) -> (Option<PathBuf>, bool, String) {
    match kind {
        TargetKind::Temp => match std::env::var("TEMP").or_else(|_| std::env::var("TMP")) {
            Ok(p) => (Some(PathBuf::from(p)), true, "locked files skipped".into()),
            Err(_) => (None, false, "TEMP env var not set".into()),
        },
        TargetKind::Npm => {
            if tool_on_path("npm") {
                if let Some(dir) = tool_output("npm", &["config", "get", "cache"], None) {
                    return (Some(PathBuf::from(dir)), true, String::new());
                }
            }
            match std::env::var("LOCALAPPDATA") {
                Ok(p) => {
                    let fallback = PathBuf::from(p).join("npm-cache");
                    if fallback.exists() {
                        (Some(fallback), true, "npm CLI not found · default dir".into())
                    } else {
                        (None, false, "npm not found".into())
                    }
                }
                Err(_) => (None, false, "npm not found".into()),
            }
        }
        TargetKind::Pip => {
            if cfg!(windows) {
                if let Ok(p) = std::env::var("LOCALAPPDATA") {
                    let cache = PathBuf::from(p).join("pip").join("cache");
                    if cache.exists() {
                        return (Some(cache), true, String::new());
                    }
                }
            } else if let Ok(home) = std::env::var("HOME") {
                let cache = PathBuf::from(home).join(".cache").join("pip");
                if cache.exists() {
                    return (Some(cache), true, String::new());
                }
            }
            (None, false, "pip cache not found".into())
        }
        TargetKind::Pnpm => {
            if !tool_on_path("pnpm") {
                return (None, false, "pnpm not on PATH".into());
            }
            if let Some(dir) = tool_output("pnpm", &["store", "path"], None) {
                return (Some(PathBuf::from(dir)), true, String::new());
            }
            match std::env::var("LOCALAPPDATA") {
                Ok(p) => (Some(PathBuf::from(p).join("pnpm-store")), true, String::new()),
                Err(_) => (None, false, "could not resolve pnpm store".into()),
            }
        }
        TargetKind::Yarn => {
            if !tool_on_path("yarn") {
                return (None, false, "yarn not on PATH".into());
            }
            match tool_output("yarn", &["cache", "dir"], None) {
                Some(dir) => (Some(PathBuf::from(dir)), true, String::new()),
                None => (None, false, "could not resolve yarn cache dir".into()),
            }
        }
        TargetKind::NuGet => match std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            Ok(p) => {
                let dir = PathBuf::from(p).join(".nuget").join("packages");
                if dir.exists() {
                    (Some(dir), true, String::new())
                } else {
                    (None, false, "NuGet packages folder not found".into())
                }
            }
            Err(_) => (None, false, "USERPROFILE/HOME not set".into()),
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RushInfo {
    path: String,
    checked: bool,
    exists: bool,
    size_bytes: Option<u64>,
    file_count: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanResult {
    targets: Vec<TargetInfo>,
    rush_repos: Vec<RushInfo>,
    drive_total: Option<u64>,
    drive_free: Option<u64>,
}

fn drive_stats() -> (Option<u64>, Option<u64>) {
    let probe = std::env::var("TEMP").unwrap_or_else(|_| {
        if cfg!(windows) { "C:\\".into() } else { "/".into() }
    });
    let root = if cfg!(windows) {
        format!("{}\\", probe.chars().take(2).collect::<String>().to_uppercase())
    } else {
        "/".to_string()
    };
    (fs2::total_space(&root).ok(), fs2::available_space(&root).ok())
}

#[tauri::command]
fn scan_targets(app: tauri::AppHandle) -> ScanResult {
    let mut targets = Vec::new();
    for (kind, label, default_checked) in BASE_TARGETS {
        let id = format!("{kind:?}");
        emit_job(&app, &id, &format!("Sniffing {label}…"));
        let (path, available, note) = resolve_base_target(*kind);
        let (bytes, files) = match &path {
            Some(p) if p.exists() => scan_dir(p),
            _ => (0, 0),
        };
        targets.push(TargetInfo {
            id,
            label: label.to_string(),
            path: path.as_ref().map(|p| p.display().to_string()),
            available,
            note,
            checked: *default_checked && available,
            size_bytes: Some(bytes),
            file_count: Some(files),
        });
    }
    let cfg = load_config();
    let rush_repos = cfg
        .rush_repos
        .iter()
        .map(|repo| {
            emit_job(&app, &format!("rush:{repo}"), "Sniffing Rush repo…");
            let temp = PathBuf::from(repo).join("common").join("temp");
            let exists = temp.exists();
            let (bytes, files) = if exists { scan_dir(&temp) } else { (0, 0) };
            RushInfo {
                path: repo.clone(),
                checked: true,
                exists,
                size_bytes: Some(bytes),
                file_count: Some(files),
            }
        })
        .collect();
    let (drive_total, drive_free) = drive_stats();
    ScanResult { targets, rush_repos, drive_total, drive_free }
}

fn clean_base_target(kind: TargetKind, path: &Option<PathBuf>, lines: &mut Vec<String>) -> (u64, u32) {
    match kind {
        TargetKind::Pip => {
            if cfg!(windows) {
                if tool_on_path("python") {
                    if let Ok(o) = command("python").args(["-m", "pip", "cache", "purge"]).output() {
                        if o.status.success() {
                            lines.push("  pip cache purged".into());
                            return (0, 0);
                        }
                        lines.push(format!("  pip purge failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                    }
                }
            }
            match path {
                Some(p) if p.exists() => purge_dir_contents(p, lines),
                _ => (0, 0),
            }
        }
        TargetKind::Pnpm => {
            if let Ok(o) = command("pnpm").args(["store", "prune"]).output() {
                if o.status.success() {
                    lines.push("  pnpm store pruned".into());
                    return (0, 0);
                }
                lines.push(format!("  pnpm store prune failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                (0, 1)
            } else {
                lines.push("  pnpm store prune failed to run".into());
                (0, 1)
            }
        }
        TargetKind::Yarn => {
            if let Ok(o) = command("yarn").args(["cache", "clean"]).output() {
                if o.status.success() {
                    lines.push("  yarn cache cleaned".into());
                    return (0, 0);
                }
                lines.push(format!("  yarn cache clean failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                (0, 1)
            } else {
                lines.push("  yarn cache clean failed to run".into());
                (0, 1)
            }
        }
        _ => match path {
            Some(p) if p.exists() => purge_dir_contents(p, lines),
            _ => (0, 0),
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanResult {
    lines: Vec<String>,
    total_freed: u64,
}

#[tauri::command]
fn clean_targets(app: tauri::AppHandle, target_ids: Vec<String>, rush_repos: Vec<String>) -> Result<CleanResult, String> {
    let mut lines = Vec::new();
    let mut total_freed = 0u64;

    let id_map: HashMap<String, TargetKind> = BASE_TARGETS
        .iter()
        .map(|(k, _, _)| (format!("{k:?}"), *k))
        .collect();

    for id in &target_ids {
        let Some(kind) = id_map.get(id) else {
            lines.push(format!("  ? unknown target {id}, skipped"));
            continue;
        };
        let (path, available, _note) = resolve_base_target(*kind);
        if !available {
            lines.push(format!("  ? {id} unavailable, skipped"));
            continue;
        }
        let label = BASE_TARGETS
            .iter()
            .find(|(k, _, _)| k == kind)
            .map(|(_, l, _)| *l)
            .unwrap_or(id.as_str());
        emit_job(&app, id, &format!("Scrubbing {label}…"));
        lines.push(format!("cleaning {label}..."));
        let (freed, errors) = clean_base_target(*kind, &path, &mut lines);
        total_freed += freed;
        // pip, pnpm, and yarn report success without a byte count. The window
        // measures what's gone by sniffing again, so don't claim "freed 0".
        let tool = matches!(kind, TargetKind::Pip | TargetKind::Pnpm | TargetKind::Yarn);
        if tool && errors == 0 {
            lines.push("  done".into());
        } else {
            lines.push(format!("  freed {}, {errors} error(s)", format_size(freed)));
        }
    }

    for repo in &rush_repos {
        emit_job(&app, &format!("rush:{repo}"), "Scrubbing Rush repo…");
        lines.push(format!("rush purge in {repo}..."));
        let temp = PathBuf::from(repo).join("common").join("temp");
        let size_hint = if temp.exists() { scan_dir(&temp).0 } else { 0 };
        if tool_on_path("rush") {
            match command("rush").arg("purge").current_dir(repo).output() {
                Ok(o) if o.status.success() => {
                    total_freed += size_hint;
                    lines.push("  rush purge ok".into());
                }
                Ok(o) => {
                    lines.push(format!("  rush purge failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                    if temp.exists() {
                        let (freed, errors) = purge_dir_contents(&temp, &mut lines);
                        total_freed += freed;
                        lines.push(format!("  fallback deleted common\\temp: freed {}, {errors} error(s)", format_size(freed)));
                    }
                }
                Err(e) => lines.push(format!("  rush purge failed: {e}")),
            }
        } else if temp.exists() {
            let (freed, errors) = purge_dir_contents(&temp, &mut lines);
            total_freed += freed;
            lines.push(format!("  rush not on PATH; deleted common\\temp: freed {}, {errors} error(s)", format_size(freed)));
        } else {
            lines.push("  no rush on PATH and common\\temp missing; skipped".into());
        }
    }

    Ok(CleanResult { lines, total_freed })
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

#[tauri::command]
fn add_rush_repo(path: String) -> Result<(), String> {
    let p = PathBuf::from(path.trim().trim_matches('"'));
    if !p.join("rush.json").exists() {
        return Err(format!("no rush.json in {}", p.display()));
    }
    let mut cfg = load_config();
    let s = p.display().to_string();
    if cfg.rush_repos.contains(&s) {
        return Err("repo already in list".into());
    }
    cfg.rush_repos.push(s);
    save_config(&cfg);
    Ok(())
}

#[tauri::command]
fn remove_rush_repo(path: String) -> Result<(), String> {
    let mut cfg = load_config();
    cfg.rush_repos.retain(|r| r != &path);
    save_config(&cfg);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_targets,
            clean_targets,
            add_rush_repo,
            remove_rush_repo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Sucky Cache Cleaner");
}
