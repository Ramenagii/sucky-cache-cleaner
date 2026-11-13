use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

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
        match serde_json::to_string_pretty(cfg) {
            Ok(json) => {
                if let Err(e) = fs::write(&p, json) {
                    eprintln!("save config failed: {e}");
                }
            }
            Err(e) => eprintln!("serialize config failed: {e}"),
        }
    }
}

fn fmt_size(bytes: u64) -> String {
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

fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn tool_on_path(tool: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    Command::new(finder)
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn tool_output(tool: &str, args: &[&str], cwd: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new(tool);
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

#[derive(Clone, Default)]
struct DirScan {
    bytes: u64,
    files: u64,
    /// extension (lowercase, no dot; "(none)" when absent) -> (count, bytes)
    by_ext: HashMap<String, (u64, u64)>,
    /// (file name, bytes), top 5 by size
    largest: Vec<(String, u64)>,
}

fn scan_dir(path: &Path) -> (u64, u64) {
    let detail = scan_dir_detail(path);
    (detail.bytes, detail.files)
}

fn scan_dir_detail(path: &Path) -> DirScan {
    let mut scan = DirScan::default();
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        scan.files += 1;
        scan.bytes += size;
        let ext = entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "(none)".to_string());
        let slot = scan.by_ext.entry(ext).or_insert((0, 0));
        slot.0 += 1;
        slot.1 += size;
        scan.largest.push((entry.file_name().to_string_lossy().to_string(), size));
        scan.largest.sort_by(|a, b| b.1.cmp(&a.1));
        scan.largest.truncate(5);
    }
    scan
}

fn gather_suggestions(rows: &[TargetRow], rush: &[RushRow]) -> Vec<(bool, String, u64)> {
    let mut v = Vec::new();
    for r in rows {
        if !r.available {
            continue;
        }
        if let Some(b) = r.size_bytes {
            if b >= 100 * 1024 * 1024 {
                let (name, reason) = match r.kind {
                    TargetKind::Temp => ("TEMP", "safe — locked files are skipped"),
                    TargetKind::Npm => ("npm cache", "safe — re-downloads on next install"),
                    TargetKind::Pip => ("pip cache", "safe — re-downloads on next install"),
                    TargetKind::Pnpm => ("pnpm store", "safe — re-downloads on next install"),
                    TargetKind::Yarn => ("yarn cache", "safe — re-downloads on next install"),
                    TargetKind::NuGet => ("NuGet packages", "safe — re-downloads on demand"),
                };
                v.push((b >= 1_000_000_000, format!("{name}: {} — {reason}", fmt_size(b)), b));
            }
        }
    }
    for r in rush {
        if let Some(b) = r.size_bytes {
            if b >= 100 * 1024 * 1024 {
                let repo_name = std::path::Path::new(&r.repo)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| r.repo.clone());
                v.push((b >= 1_000_000_000, format!("{repo_name} common\\temp: {} — regenerated on next build", fmt_size(b)), b));
            }
        }
    }
    v.sort_by(|a, b| b.2.cmp(&a.2));
    v
}

fn top_extensions(scan: &DirScan, n: usize) -> Vec<(String, u64, u64)> {
    let mut v: Vec<(String, u64, u64)> = scan
        .by_ext
        .iter()
        .map(|(k, (c, b))| (k.clone(), *c, *b))
        .collect();
    v.sort_by(|a, b| b.2.cmp(&a.2));
    v.truncate(n);
    v
}

fn purge_dir_contents(root: &Path, log: &mut Vec<String>) -> (u64, u32) {
    let mut freed = 0u64;
    let mut errors = 0u32;
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(e) => {
            log.push(format!("  ! cannot read {}: {e}", root.display()));
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
                log.push(format!("  ! skip {}: {e}", path.display()));
            }
        }
    }
    (freed, errors)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum TargetKind {
    Temp,
    Npm,
    Pip,
    Pnpm,
    Yarn,
    NuGet,
}

const BASE_TARGETS: &[(TargetKind, &str, bool)] = &[
    (TargetKind::Temp, "User TEMP folder (locked files skipped)", false),
    (TargetKind::Npm, "npm cache", true),
    (TargetKind::Pip, "pip cache", true),
    (TargetKind::Pnpm, "pnpm store", true),
    (TargetKind::Yarn, "yarn cache", true),
    (TargetKind::NuGet, "NuGet packages (global packages folder)", false),
];

#[derive(Clone)]
struct TargetRow {
    kind: TargetKind,
    label: String,
    path: Option<PathBuf>,
    available: bool,
    note: String,
    checked: bool,
    size_bytes: Option<u64>,
    file_count: Option<u64>,
    detail: Option<DirScan>,
}

struct RushRow {
    repo: String,
    checked: bool,
    size_bytes: Option<u64>,
    file_count: Option<u64>,
    detail: Option<DirScan>,
    note: String,
}

enum WorkerMsg {
    ScanDone {
        bases: HashMap<TargetKind, (Option<PathBuf>, bool, String, DirScan)>,
        rush: Vec<(String, DirScan, String)>,
    },
    CleanDone {
        lines: Vec<String>,
        total_freed: u64,
    },
}

fn resolve_base_target(kind: TargetKind) -> (Option<PathBuf>, bool, String) {
    match kind {
        TargetKind::Temp => match std::env::var("TEMP").or_else(|_| std::env::var("TMP")) {
            Ok(p) => (Some(PathBuf::from(p)), true, String::new()),
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
                        (Some(fallback), true, "npm CLI not found; using default cache dir".into())
                    } else {
                        (None, false, "npm not found and default cache dir missing".into())
                    }
                }
                Err(_) => (None, false, "npm not found".into()),
            }
        }
        TargetKind::Pip => match std::env::var("LOCALAPPDATA") {
            Ok(p) => {
                let cache = PathBuf::from(p).join("pip").join("cache");
                if cache.exists() {
                    (Some(cache), true, String::new())
                } else if tool_on_path("python") {
                    (Some(PathBuf::new()), true, "cache dir absent; will run pip cache purge".into())
                } else {
                    (None, false, "no pip cache found".into())
                }
            }
            Err(_) => (None, false, "LOCALAPPDATA not set".into()),
        },
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
        TargetKind::NuGet => match std::env::var("USERPROFILE") {
            Ok(p) => {
                let dir = PathBuf::from(p).join(".nuget").join("packages");
                if dir.exists() {
                    (Some(dir), true, String::new())
                } else {
                    (None, false, "NuGet packages folder not found".into())
                }
            }
            Err(_) => (None, false, "USERPROFILE not set".into()),
        },
    }
}

fn clean_base_target(kind: TargetKind, path: &Option<PathBuf>, log: &mut Vec<String>) -> (u64, u32) {
    match kind {
        TargetKind::Pip => {
            // Prefer pip's own purge; fall back to deleting the cache dir.
            if tool_on_path("python") {
                let out = Command::new("python")
                    .args(["-m", "pip", "cache", "purge"])
                    .output();
                match out {
                    Ok(o) if o.status.success() => {
                        let msg = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        log.push(format!("  pip: {msg}"));
                        let freed = path.as_ref().filter(|p| !p.as_os_str().is_empty())
                            .map(|p| p.join("..").canonicalize().ok().map(|_| 0).unwrap_or(0))
                            .unwrap_or(0);
                        return (freed, 0);
                    }
                    Ok(o) => {
                        log.push(format!("  pip purge failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                    }
                    Err(e) => log.push(format!("  pip purge failed: {e}")),
                }
            }
            match path {
                Some(p) if !p.as_os_str().is_empty() => purge_dir_contents(p, log),
                _ => (0, 0),
            }
        }
        TargetKind::Pnpm => {
            match Command::new("pnpm").args(["store", "prune"]).output() {
                Ok(o) if o.status.success() => {
                    log.push("  pnpm store pruned".into());
                    (0, 0)
                }
                Ok(o) => {
                    log.push(format!("  pnpm store prune failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                    (0, 1)
                }
                Err(e) => {
                    log.push(format!("  pnpm store prune failed: {e}"));
                    (0, 1)
                }
            }
        }
        TargetKind::Yarn => {
            match Command::new("yarn").args(["cache", "clean"]).output() {
                Ok(o) if o.status.success() => {
                    log.push("  yarn cache cleaned".into());
                    (0, 0)
                }
                Ok(o) => {
                    log.push(format!("  yarn cache clean failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                    (0, 1)
                }
                Err(e) => {
                    log.push(format!("  yarn cache clean failed: {e}"));
                    (0, 1)
                }
            }
        }
        _ => match path {
            Some(p) if !p.as_os_str().is_empty() => purge_dir_contents(p, log),
            _ => (0, 0),
        },
    }
}

const CREAM: egui::Color32 = egui::Color32::from_rgb(0xED, 0xED, 0xE7);
const PANEL: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF);
const PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xC2, 0x41, 0x0C);
const LEAF: egui::Color32 = egui::Color32::from_rgb(0x2F, 0xA8, 0x4F);
const TEXT: egui::Color32 = egui::Color32::from_rgb(0x14, 0x14, 0x14);
const MUTED: egui::Color32 = egui::Color32::from_rgb(0x7F, 0x7E, 0x77);
const AMBER: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x9E, 0x00);
const HAIRLINE: egui::Color32 = egui::Color32::from_rgb(0xE0, 0xE0, 0xDA);

fn buddy_card() -> egui::Frame {
    egui::Frame {
        fill: PANEL,
        corner_radius: egui::CornerRadius::same(24),
        inner_margin: egui::Margin::same(16),
        stroke: egui::Stroke::new(1.0, HAIRLINE),
        shadow: egui::epaint::Shadow { offset: [0, 4], blur: 22, spread: 0, color: egui::Color32::from_black_alpha(26) },
        ..egui::Frame::none()
    }
}

/// Cartoon fish mascot drawn from shapes: ellipse body, tail, fin, eye, smile.
fn paint_fish(ui: &mut egui::Ui, center: egui::Pos2, scale: f32) {
    let painter = ui.painter().clone();
    let body = egui::Vec2::new(46.0 * scale, 34.0 * scale);
    // tail
    let tail = vec![
        center + egui::vec2(-40.0 * scale, 0.0),
        center + egui::vec2(-64.0 * scale, -18.0 * scale),
        center + egui::vec2(-64.0 * scale, 18.0 * scale),
    ];
    painter.add(egui::Shape::convex_polygon(tail, egui::Color32::from_rgb(0xB0, 0x72, 0x2F), egui::Stroke::NONE));
    // body
    painter.add(egui::Shape::ellipse_filled(center, body, egui::Color32::from_rgb(0xC9, 0x8A, 0x3F)));
    // spots
    for (dx, dy, r) in [(-14.0_f32, -6.0_f32, 4.0_f32), (8.0, 4.0, 3.2), (-2.0, -12.0, 2.4), (16.0, -8.0, 2.8), (-20.0, 8.0, 2.6)] {
        painter.circle_filled(center + egui::vec2(dx * scale, dy * scale), r * scale, egui::Color32::from_rgb(0x8A, 0x5A, 0x24));
    }
    // dorsal fin
    let fin = vec![
        center + egui::vec2(-6.0 * scale, -30.0 * scale),
        center + egui::vec2(8.0 * scale, -44.0 * scale),
        center + egui::vec2(20.0 * scale, -28.0 * scale),
    ];
    painter.add(egui::Shape::convex_polygon(fin, egui::Color32::from_rgb(0xB0, 0x72, 0x2F), egui::Stroke::NONE));
    // apron
    let apron = vec![
        center + egui::vec2(6.0 * scale, 4.0 * scale),
        center + egui::vec2(34.0 * scale, 4.0 * scale),
        center + egui::vec2(30.0 * scale, 26.0 * scale),
        center + egui::vec2(10.0 * scale, 26.0 * scale),
    ];
    painter.add(egui::Shape::convex_polygon(apron, PRIMARY, egui::Stroke::NONE));
    // eye
    painter.circle_filled(center + egui::vec2(26.0 * scale, -8.0 * scale), 6.5 * scale, egui::Color32::WHITE);
    painter.circle_filled(center + egui::vec2(27.5 * scale, -8.0 * scale), 3.4 * scale, TEXT);
    painter.circle_filled(center + egui::vec2(28.6 * scale, -9.2 * scale), 1.1 * scale, egui::Color32::WHITE);
    // smile
    let smile: Vec<egui::Pos2> = (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            let x = 10.0 + t * 22.0;
            let y = 6.0 + (t * std::f32::consts::PI).sin() * 5.0;
            center + egui::vec2(x * scale, y * scale)
        })
        .collect();
    painter.add(egui::Shape::line(smile, egui::Stroke::new(2.0, TEXT)));
}

fn paint_buddy_bar(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), egui::Sense::hover());
    let cy = rect.center().y;
    paint_fish(ui, egui::pos2(rect.left() + 38.0, cy), 0.42);
    let painter = ui.painter_at(rect);
    painter.text(
        egui::pos2(rect.left() + 74.0, cy - 6.0),
        egui::Align2::LEFT_CENTER,
        "Sucky",
        egui::FontId::proportional(22.0),
        TEXT,
    );
    painter.text(
        egui::pos2(rect.left() + 74.0, cy + 10.0),
        egui::Align2::LEFT_CENTER,
        "your cleanup buddy",
        egui::FontId::proportional(11.0),
        MUTED,
    );
    painter.line_segment(
        [egui::pos2(rect.left(), rect.bottom()), egui::pos2(rect.right(), rect.bottom())],
        egui::Stroke::new(1.0, HAIRLINE),
    );
}

struct App {
    rows: Vec<TargetRow>,
    rush_rows: Vec<RushRow>,
    repo_input: String,
    log: Vec<String>,
    busy: bool,
    show_confirm: bool,
    styled: bool,
    drive_total: Option<u64>,
    drive_free: Option<u64>,
    anim_selected: f32,
    anim_drive: f32,
    toast: Option<(String, std::time::Instant)>,
    tx: Sender<WorkerMsg>,
    rx: Receiver<WorkerMsg>,
}

impl App {
    fn refresh_drive_stats(&mut self) {
        // Measure the drive that hosts the user temp dir (usually C:).
        let probe = std::env::var("TEMP").unwrap_or_else(|_| "C:\\".into());
        let root = probe
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase()
            + "\\";
        self.drive_total = fs2::total_space(&root).ok();
        self.drive_free = fs2::available_space(&root).ok();
    }

    fn new() -> Self {
        let (tx, rx) = channel();
        let probe = std::env::var("TEMP").unwrap_or_else(|_| "C:\\".into());
        let drive_root = probe.chars().take(2).collect::<String>().to_uppercase() + "\\";
        let drive_total = fs2::total_space(&drive_root).ok();
        let drive_free = fs2::available_space(&drive_root).ok();
        let cfg = load_config();
        let rows = BASE_TARGETS
            .iter()
            .map(|(kind, label, default_checked)| {
                let (path, available, note) = resolve_base_target(*kind);
                TargetRow {
                    kind: *kind,
                    label: label.to_string(),
                    path,
                    available,
                    note,
                    checked: *default_checked,
                    size_bytes: None,
                    file_count: None,
                    detail: None,
                }
            })
            .collect();
        let rush_rows = cfg
            .rush_repos
            .iter()
            .map(|r| RushRow {
                repo: r.clone(),
                checked: true,
                size_bytes: None,
                file_count: None,
                detail: None,
                note: String::new(),
            })
            .collect();
        Self {
            rows,
            rush_rows,
            repo_input: String::new(),
            log: vec![format!("[{}] ready. Click Scan to measure, Clean to purge.", timestamp())],
            busy: false,
            show_confirm: false,
            styled: false,
            drive_total,
            drive_free,
            anim_selected: 0.0,
            anim_drive: 0.0,
            toast: None,
            tx,
            rx,
        }
    }

    fn push_log(&mut self, line: String) {
        self.log.push(format!("[{}] {line}", timestamp()));
        if self.log.len() > 500 {
            let drain = self.log.len() - 500;
            self.log.drain(0..drain);
        }
    }

    fn start_scan(&mut self) {
        self.busy = true;
        let rush_repos: Vec<String> = self.rush_rows.iter().map(|r| r.repo.clone()).collect();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let mut bases = HashMap::new();
            for (kind, _, _) in BASE_TARGETS {
                let (path, available, note) = resolve_base_target(*kind);
                let detail = match &path {
                    Some(p) if p.exists() => scan_dir_detail(p),
                    _ => DirScan::default(),
                };
                bases.insert(*kind, (path, available, note, detail));
            }
            let rush = rush_repos
                .into_iter()
                .map(|repo| {
                    let temp = PathBuf::from(&repo).join("common").join("temp");
                    if temp.exists() {
                        (repo, scan_dir_detail(&temp), String::new())
                    } else {
                        (repo, DirScan::default(), "common\\temp missing".into())
                    }
                })
                .collect();
            let _ = tx.send(WorkerMsg::ScanDone { bases, rush });
        });
    }

    fn start_clean(&mut self) {
        self.busy = true;
        self.show_confirm = false;
        let work: Vec<(TargetKind, Option<PathBuf>)> = self
            .rows
            .iter()
            .filter(|r| r.checked && r.available)
            .map(|r| (r.kind, r.path.clone()))
            .collect();
        let rush_work: Vec<(String, u64)> = self
            .rush_rows
            .iter()
            .filter(|r| r.checked)
            .map(|r| (r.repo.clone(), r.size_bytes.unwrap_or(0)))
            .collect();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let mut lines = Vec::new();
            let mut total_freed = 0u64;
            for (kind, path) in work {
                lines.push(format!("cleaning {kind:?}..."));
                let (freed, errors) = clean_base_target(kind, &path, &mut lines);
                total_freed += freed;
                lines.push(format!("  freed {}, {errors} error(s)", fmt_size(freed)));
            }
            for (repo, size_hint) in rush_work {
                lines.push(format!("rush purge in {repo}..."));
                let temp = PathBuf::from(&repo).join("common").join("temp");
                if tool_on_path("rush") {
                    match Command::new("rush").arg("purge").current_dir(&repo).output() {
                        Ok(o) if o.status.success() => {
                            total_freed += size_hint;
                            lines.push("  rush purge ok".into());
                        }
                        Ok(o) => {
                            lines.push(format!("  rush purge failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                            if temp.exists() {
                                let (freed, errors) = purge_dir_contents(&temp, &mut lines);
                                total_freed += freed;
                                lines.push(format!("  fallback deleted common\\temp: freed {}, {errors} error(s)", fmt_size(freed)));
                            }
                        }
                        Err(e) => lines.push(format!("  rush purge failed: {e}")),
                    }
                } else if temp.exists() {
                    let (freed, errors) = purge_dir_contents(&temp, &mut lines);
                    total_freed += freed;
                    lines.push(format!("  rush not on PATH; deleted common\\temp: freed {}, {errors} error(s)", fmt_size(freed)));
                } else {
                    lines.push("  no rush on PATH and common\\temp missing; skipped".into());
                }
            }
            let _ = tx.send(WorkerMsg::CleanDone { lines, total_freed });
        });
    }

    fn drain_worker(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                WorkerMsg::ScanDone { bases, rush } => {
                    for row in &mut self.rows {
                        if let Some((path, available, note, detail)) = bases.get(&row.kind) {
                            row.path = path.clone();
                            row.available = *available;
                            row.note = note.clone();
                            row.size_bytes = Some(detail.bytes);
                            row.file_count = Some(detail.files);
                            row.detail = Some(detail.clone());
                        }
                    }
                    for (repo, detail, note) in rush {
                        if let Some(row) = self.rush_rows.iter_mut().find(|r| r.repo == repo) {
                            row.size_bytes = Some(detail.bytes);
                            row.file_count = Some(detail.files);
                            row.detail = Some(detail);
                            row.note = note;
                        }
                    }
                    self.refresh_drive_stats();
                    self.push_log("scan complete".into());
                    self.busy = false;
                }
                WorkerMsg::CleanDone { lines, total_freed } => {
                    for line in lines {
                        self.push_log(line);
                    }
                    self.toast = Some((format!("Sucky scrubbed {} of junk", fmt_size(total_freed)), std::time::Instant::now()));
                    self.busy = false;
                    self.start_scan();
                }
            }
        }
    }

    fn mark_all_suggested(&mut self) {
        let mut total = 0u64;
        for row in &mut self.rows {
            if row.available && row.size_bytes.map(|b| b >= 100 * 1024 * 1024).unwrap_or(false) {
                row.checked = true;
                total += row.size_bytes.unwrap_or(0);
            }
        }
        for row in &mut self.rush_rows {
            if row.size_bytes.map(|b| b >= 100 * 1024 * 1024).unwrap_or(false) {
                row.checked = true;
                total += row.size_bytes.unwrap_or(0);
            }
        }
        self.toast = Some((format!("Marked {} — hit Clean when ready", fmt_size(total)), std::time::Instant::now()));
    }

    fn checked_total_bytes(&self) -> u64 {
        let base: u64 = self
            .rows
            .iter()
            .filter(|r| r.checked)
            .filter_map(|r| r.size_bytes)
            .sum();
        let rush: u64 = self
            .rush_rows
            .iter()
            .filter(|r| r.checked)
            .filter_map(|r| r.size_bytes)
            .sum();
        base + rush
    }

    fn persist_repos(&self) {
        let cfg = Config {
            rush_repos: self.rush_rows.iter().map(|r| r.repo.clone()).collect(),
        };
        save_config(&cfg);
    }

    fn add_repo(&mut self) {
        let path = self.repo_input.trim().trim_matches('"').to_string();
        if path.is_empty() {
            return;
        }
        let p = PathBuf::from(&path);
        if !p.join("rush.json").exists() {
            self.push_log(format!("add repo failed: no rush.json in {path}"));
            return;
        }
        if self.rush_rows.iter().any(|r| r.repo == path) {
            self.push_log("repo already in list".into());
            return;
        }
        self.rush_rows.push(RushRow {
            repo: path.clone(),
            checked: true,
            size_bytes: None,
            file_count: None,
            detail: None,
            note: String::new(),
        });
        self.repo_input.clear();
        self.persist_repos();
        self.push_log(format!("added rush repo {path}"));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_worker();
        if !self.styled {
            let mut visuals = egui::Visuals::light();
            visuals.panel_fill = CREAM;
            visuals.window_fill = CREAM;
            visuals.window_corner_radius = egui::CornerRadius::same(26);
            visuals.window_stroke = egui::Stroke::new(1.0, HAIRLINE);
            visuals.selection.bg_fill = PRIMARY;
            visuals.extreme_bg_color = PANEL;
            visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, HAIRLINE);
            visuals.widgets.noninteractive.bg_fill = PANEL;
            visuals.widgets.inactive.bg_fill = PANEL;
            visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0xF3, 0xEE, 0xDD);
            visuals.widgets.active.bg_fill = PRIMARY;
            visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
            visuals.widgets.open.bg_fill = PANEL;
            ctx.set_visuals(visuals);
            self.styled = true;
        }
        // Animated values: ease selection total and drive fill toward targets.
        let target_sel = self.checked_total_bytes() as f32;
        let diff = target_sel - self.anim_selected;
        if diff.abs() > 1.0 {
            self.anim_selected += diff * 0.18;
            ctx.request_repaint();
        } else {
            self.anim_selected = target_sel;
        }
        let drive_frac = match (self.drive_total, self.drive_free) {
            (Some(t), Some(f)) if t > 0 => (t.saturating_sub(f)) as f32 / t as f32,
            _ => 0.0,
        };
        let dd = drive_frac - self.anim_drive;
        if dd.abs() > 0.002 {
            self.anim_drive += dd * 0.08;
            ctx.request_repaint();
        } else {
            self.anim_drive = drive_frac;
        }

        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        egui::TopBottomPanel::top("titlebar").frame(egui::Frame::none().fill(CREAM)).show(ctx, |ui| {
            paint_buddy_bar(ui);
        });

        egui::TopBottomPanel::bottom("logpane")
            .resizable(true)
            .default_height(160.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.strong("Log");
                    if ui.small_button("Clear").clicked() {
                        self.log.clear();
                    }
                });
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.monospace(line);
                        }
                    });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(CREAM).inner_margin(egui::Margin::same(14)))
            .show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                // Left: headline + actions
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        paint_fish(ui, ui.cursor().min + egui::vec2(16.0, 12.0), 0.5);
                        ui.add_space(44.0);
                        ui.heading(egui::RichText::new("A clean tank,\na clean disk.").size(34.0).strong().color(TEXT));
                    });
                    ui.label(egui::RichText::new("Sucky hunts dev caches, temp files and Rush debris.\nOne scrub and it's gone.").size(13.0).color(MUTED));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let scan = ui.add_enabled(
                            !self.busy,
                            egui::Button::new(egui::RichText::new("  Scan my disk  ").strong().size(15.0).color(TEXT))
                                .fill(PANEL)
                                .corner_radius(egui::CornerRadius::same(20)),
                        );
                        if scan.clicked() {
                            self.start_scan();
                        }
                        let clean_label = format!("  Clean {:.1} GB  ", self.anim_selected as f64 / 1e9);
                        let clean = ui.add_enabled(
                            !self.busy,
                            egui::Button::new(egui::RichText::new(clean_label).strong().size(17.0).color(egui::Color32::WHITE))
                                .fill(PRIMARY)
                                .corner_radius(egui::CornerRadius::same(22))
                                .min_size(egui::vec2(0.0, 40.0)),
                        );
                        if clean.clicked() {
                            self.show_confirm = true;
                        }
                    });
                    if self.busy {
                        ui.add_space(6.0);
                        ui.add(egui::ProgressBar::new(1.0).animate(true).desired_width(260.0).fill(LEAF).corner_radius(egui::CornerRadius::same(6)));
                        ui.spinner();
                        ui.weak("scrub-a-dub…");
                    }
                });

                // Right: TANK STATUS card
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    buddy_card().show(ui, |ui| {
                        ui.set_min_width(230.0);
                        ui.label(egui::RichText::new("TANK STATUS").small().strong().color(MUTED));
                        ui.add_space(2.0);
                        let pct = (self.anim_drive * 100.0) as u32;
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("{pct}%")).size(38.0).strong().color(PRIMARY));
                            ui.label(egui::RichText::new("full").color(MUTED));
                        });
                        ui.add(
                            egui::ProgressBar::new(self.anim_drive)
                                .desired_width(210.0)
                                .fill(PRIMARY)
                                .corner_radius(egui::CornerRadius::same(6)),
                        );
                        ui.add_space(8.0);
                        let total_junk: u64 = self.rows.iter().filter_map(|r| r.size_bytes).sum::<u64>()
                            + self.rush_rows.iter().filter_map(|r| r.size_bytes).sum::<u64>();
                        let suggested: u64 = self.rows.iter().filter(|r| r.available).filter_map(|r| r.size_bytes).filter(|b| *b >= 100 * 1024 * 1024).sum::<u64>()
                            + self.rush_rows.iter().filter_map(|r| r.size_bytes).filter(|b| *b >= 100 * 1024 * 1024).sum::<u64>();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Junk found").color(MUTED));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.strong(fmt_size(total_junk));
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Suggested clean").color(MUTED));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.strong(egui::RichText::new(fmt_size(suggested)).color(PRIMARY));
                            });
                        });
                        if let Some(free) = self.drive_free {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Drive free").color(MUTED));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new(fmt_size(free)).strong());
                                });
                            });
                        }
                    });
                });
            });
            ui.add_space(10.0);

            // Sucky spotted this — suggestions
            let suggestions = gather_suggestions(&self.rows, &self.rush_rows);
            if !suggestions.is_empty() {
                buddy_card().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("SUCKY SPOTTED THIS").small().strong().color(MUTED));
                        let total: u64 = suggestions.iter().map(|s| s.2).sum();
                        if ui
                            .add(egui::Button::new(egui::RichText::new(format!("Mark all suggested ({})", fmt_size(total))).small().strong().color(egui::Color32::WHITE))
                                .fill(LEAF)
                                .corner_radius(egui::CornerRadius::same(14)))
                            .clicked()
                        {
                            self.mark_all_suggested();
                        }
                    });
                    ui.add_space(6.0);
                    for (big, text, _) in &suggestions {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 6.5, if *big { AMBER } else { LEAF });
                            ui.label(egui::RichText::new(text).color(TEXT));
                        });
                        ui.add_space(2.0);
                    }
                });
                ui.add_space(8.0);
            }

            buddy_card().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(egui::RichText::new("CACHES").small().strong().color(MUTED));
                ui.add_space(6.0);
                for row in &mut self.rows {
                    ui.horizontal(|ui| {
                        ui.add_enabled(row.available, egui::Checkbox::new(&mut row.checked, ""));
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&row.label).color(egui::Color32::from_rgb(0x1D, 0x1D, 0x1F)));
                            if let Some(p) = &row.path {
                                ui.label(egui::RichText::new(p.display().to_string()).small().color(egui::Color32::from_rgb(0x8E, 0x8E, 0x93)));
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match (row.size_bytes, row.file_count) {
                                (Some(b), Some(f)) => ui.label(egui::RichText::new(format!("{}  ·  {f} files", fmt_size(b))).color(egui::Color32::from_rgb(0x1D, 0x1D, 0x1F))),
                                _ => ui.label("—"),
                            };
                        });
                    });
                    if !row.note.is_empty() || !row.available {
                        ui.label(egui::RichText::new(&row.note).small().color(egui::Color32::from_rgb(0xFF, 0x3B, 0x30)));
                    }
                    if let Some(detail) = &row.detail {
                        if detail.files > 0 {
                            egui::CollapsingHeader::new(egui::RichText::new("what will be deleted").small().color(egui::Color32::from_rgb(0x00, 0x7A, 0xFF)))
                                .id_source(("detail", row.kind as u8))
                                .show(ui, |ui| {
                                    let exts = top_extensions(detail, 4);
                                    if !exts.is_empty() {
                                        let parts: Vec<String> = exts
                                            .into_iter()
                                            .map(|(e, c, b)| {
                                                let name = if e == "(none)" { "no ext".to_string() } else { format!(".{e}") };
                                                format!("{name} {} · {c} files", fmt_size(b))
                                            })
                                            .collect();
                                        ui.label(egui::RichText::new(format!("top types: {}", parts.join("   "))).small());
                                    }
                                    for (name, size) in detail.largest.iter().take(3) {
                                        ui.label(egui::RichText::new(format!("largest: {name}  ({})", fmt_size(*size))).small());
                                    }
                                });
                        }
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }

                ui.add_space(4.0);
                ui.label(egui::RichText::new("RUSH REPOS — `rush purge`").small().strong().color(egui::Color32::from_rgb(0x8E, 0x8E, 0x93)));
                ui.add_space(6.0);
                let mut rush_remove: Option<usize> = None;
                for (idx, row) in self.rush_rows.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut row.checked, "");
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&row.repo).color(egui::Color32::from_rgb(0x1D, 0x1D, 0x1F)));
                            if !row.note.is_empty() {
                                ui.label(egui::RichText::new(&row.note).small().color(egui::Color32::from_rgb(0xFF, 0x3B, 0x30)));
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Remove").clicked() {
                                rush_remove = Some(idx);
                            }
                            match (row.size_bytes, row.file_count) {
                                (Some(b), Some(f)) => ui.label(egui::RichText::new(format!("{}  ·  {f} files", fmt_size(b))).color(egui::Color32::from_rgb(0x1D, 0x1D, 0x1F))),
                                _ => ui.label("—"),
                            };
                        });
                    });
                    if let Some(detail) = &row.detail {
                        if detail.files > 0 {
                            egui::CollapsingHeader::new(egui::RichText::new("what will be deleted").small().color(egui::Color32::from_rgb(0x00, 0x7A, 0xFF)))
                                .id_source(("rush-detail", idx))
                                .show(ui, |ui| {
                                    let exts = top_extensions(detail, 4);
                                    if !exts.is_empty() {
                                        let parts: Vec<String> = exts
                                            .into_iter()
                                            .map(|(e, c, b)| {
                                                let name = if e == "(none)" { "no ext".to_string() } else { format!(".{e}") };
                                                format!("{name} {} · {c} files", fmt_size(b))
                                            })
                                            .collect();
                                        ui.label(egui::RichText::new(format!("top types: {}", parts.join("   "))).small());
                                    }
                                    for (name, size) in detail.largest.iter().take(3) {
                                        ui.label(egui::RichText::new(format!("largest: {name}  ({})", fmt_size(*size))).small());
                                    }
                                });
                        }
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                if let Some(idx) = rush_remove {
                    let removed = self.rush_rows.remove(idx);
                    self.push_log(format!("removed repo {}", removed.repo));
                    self.persist_repos();
                }

                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.repo_input)
                            .hint_text("C:\\path\\to\\rush-repo")
                            .desired_width(360.0),
                    );
                    if ui.button("Add Rush repo").clicked()
                        || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        self.add_repo();
                    }
                });
            });
            });
        });

        // Toast — fades out after ~4s
        if let Some((msg, created)) = &self.toast {
            let elapsed = created.elapsed().as_secs_f32();
            if elapsed < 4.0 {
                let alpha = (1.0 - (elapsed / 4.0).powi(3)).clamp(0.0, 1.0);
                ctx.request_repaint();
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -84.0])
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(TEXT.gamma_multiply(alpha))
                            .corner_radius(egui::CornerRadius::same(18))
                            .inner_margin(egui::Margin::symmetric(18, 10))
                            .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 18, spread: 0, color: egui::Color32::from_black_alpha(40) })
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(msg).strong().color(egui::Color32::WHITE.gamma_multiply(alpha)));
                            });
                    });
            } else {
                self.toast = None;
            }
        }

        if self.show_confirm {
            egui::Window::new("Confirm clean")
                .collapsible(false)
                .resizable(false)
                .frame(buddy_card())
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("Sucky is about to scrub these. This can't be undone:").color(TEXT));
                    ui.add_space(6.0);
                    for row in self.rows.iter().filter(|r| r.checked && r.available) {
                        ui.label(egui::RichText::new(format!("• {}", row.label)).color(TEXT));
                    }
                    for row in self.rush_rows.iter().filter(|r| r.checked) {
                        ui.label(egui::RichText::new(format!("• rush purge: {}", row.repo)).color(TEXT));
                    }
                    ui.add_space(6.0);
                    ui.strong(format!("Total selected: {}", fmt_size(self.checked_total_bytes())));
                    if let Some(free) = self.drive_free {
                        ui.label(egui::RichText::new(format!("Drive free now: {}", fmt_size(free))).color(MUTED));
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("  Scrub it  ").strong().size(16.0).color(egui::Color32::WHITE)).fill(PRIMARY).corner_radius(egui::CornerRadius::same(18)).min_size(egui::vec2(0.0, 36.0))).clicked() {
                            self.start_clean();
                        }
                        if ui.add(egui::Button::new("Keep").corner_radius(egui::CornerRadius::same(18))).clicked() {
                            self.show_confirm = false;
                        }
                    });
                });
        }
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Sucky — Cache Cleaner")
            .with_inner_size([940.0, 660.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Sucky",
        native_options,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert("baloo800".to_owned(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/baloo2-800.ttf"))));
            fonts.font_data.insert("baloo700".to_owned(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/baloo2-700.ttf"))));
            let proportional = fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap();
            proportional.clear();
            proportional.insert(0, "baloo800".to_owned());
            proportional.push("baloo700".to_owned());
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(App::new()))
        }),
    )
}
