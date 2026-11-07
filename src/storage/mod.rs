//! Metadata-only, bounded scanning. Never follows reparse points or reads file contents.
mod plan;
#[cfg(test)]
mod tests;
pub use plan::Strategy;
use serde::Serialize;
use std::{
    collections::{BinaryHeap, HashMap},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

#[derive(Debug)]
pub struct Options {
    pub root: PathBuf,
    pub seconds: u64,
    pub max_entries: u64,
    pub target: Option<PathBuf>,
    pub json: bool,
}
#[derive(Debug, Serialize)]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    pub category: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Scan {
    pub schema_version: u32,
    pub root: String,
    pub unix_seconds: u64,
    pub elapsed_seconds: f64,
    pub complete: bool,
    pub stop_reason: String,
    pub entries_visited: u64,
    pub files: u64,
    pub logical_bytes: u64,
    pub skipped_reparse_or_cloud: u64,
    pub unreadable_entries: u64,
    pub error_examples: Vec<String>,
    pub directories: Vec<Entry>,
    pub largest_files: Vec<Entry>,
    pub drives: Vec<crate::model::Disk>,
    pub strategies: Vec<Strategy>,
    pub limitations: Vec<String>,
}
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        root: PathBuf::from("C:\\"),
        seconds: 60,
        max_entries: 300_000,
        target: None,
        json: false,
    };
    let mut i = 0;
    while i < args.len() {
        let key = &args[i];
        if key == "--json" {
            options.json = true;
            i += 1;
            continue;
        }
        let value = args.get(i + 1).ok_or_else(|| format!("{key} 缺少值"))?;
        match key.as_str() {
            "--path" => options.root = PathBuf::from(value),
            "--target" => options.target = Some(PathBuf::from(value)),
            "--seconds" => {
                options.seconds = value.parse().map_err(|_| "--seconds 需要整数")?;
                if !(1..=3600).contains(&options.seconds) {
                    return Err("扫描时限为 1–3600 秒".into());
                }
            }
            "--max-entries" => {
                options.max_entries = value.parse().map_err(|_| "--max-entries 需要整数")?;
                if !(1..=5_000_000).contains(&options.max_entries) {
                    return Err("扫描条目上限为 1–5000000".into());
                }
            }
            _ => return Err(format!("未知扫描参数：{key}")),
        }
        i += 2;
    }
    Ok(options)
}
fn skip(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & (0x400 | 0x1000 | 0x40000 | 0x400000) != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
fn category(path: &Path) -> &'static str {
    let s = path.to_string_lossy().replace('/', "\\").to_lowercase();
    if s.contains("\\windows")
        || s.contains("\\program files")
        || s.contains("\\programdata")
        || s.contains("\\system volume information")
        || s.ends_with("pagefile.sys")
        || s.ends_with("hiberfil.sys")
        || s.ends_with("swapfile.sys")
        || s.contains("\\recovery")
    {
        "系统与应用"
    } else if s.contains("\\node_modules")
        || s.contains("\\.cargo")
        || s.contains("\\.nuget")
        || s.contains("\\pip\\cache")
        || s.contains("\\npm-cache")
        || s.contains("\\.venv")
    {
        "开发依赖与缓存"
    } else if s.contains("\\temp") || s.contains("\\cache") || s.contains("\\$recycle.bin") {
        "缓存与临时文件"
    } else if s.contains("\\appdata") {
        "应用数据"
    } else if [
        "\\downloads",
        "\\documents",
        "\\videos",
        "\\pictures",
        "\\desktop",
        "\\music",
    ]
    .iter()
    .any(|p| s.contains(p))
    {
        "个人文件"
    } else {
        "其他文件"
    }
}
fn record_error(report: &mut Scan, path: &Path, error: &std::io::Error) {
    report.unreadable_entries += 1;
    if report.error_examples.len() < 12 {
        report
            .error_examples
            .push(format!("{}：{}", path.display(), error));
    }
}
pub fn scan(options: &Options) -> Result<Scan, String> {
    let root = fs::canonicalize(&options.root).map_err(|e| format!("无法打开扫描目录：{e}"))?;
    let original = fs::symlink_metadata(&options.root).map_err(|e| e.to_string())?;
    if skip(&original) || !original.is_dir() {
        return Err("扫描根目录必须是本地普通目录，不能是目录联接或云端占位目录".into());
    }
    #[cfg(windows)]
    if root.to_string_lossy().starts_with("\\\\?\\UNC\\") {
        return Err("扫描仅支持本地磁盘".into());
    }
    let started = Instant::now();
    let mut report = Scan { schema_version: 1, root: root.display().to_string(), unix_seconds: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs(), elapsed_seconds: 0.0, complete: true, stop_reason: "遍历完成".into(), entries_visited: 0, files: 0, logical_bytes: 0, skipped_reparse_or_cloud: 0, unreadable_entries: 0, error_examples: Vec::new(), directories: Vec::new(), largest_files: Vec::new(), drives: Vec::new(), strategies: Vec::new(), limitations: vec!["仅统计已读文件的逻辑长度；硬链接可能重复计数，稀疏/压缩文件与实际占用不同，数字不等于可回收空间。".into(),"跳过重解析点、目录联接、离线及云占位条目，不读取文件内容。不含卷影副本、NTFS 元数据与权限不可读内容。扫描期间文件可能变化。".into()] };
    #[cfg(windows)]
    {
        report.drives = crate::platform::windows::fixed_disks();
    }
    let mut totals: HashMap<PathBuf, (u64, u64)> = HashMap::new();
    // Breadth first within a priority: user data is useful even when a whole-volume scan is bounded.
    let priority = |path: &Path| match category(path) {
        "系统与应用" => 0u8,
        "应用数据" => 1,
        _ => 2,
    };
    let mut stack = BinaryHeap::new();
    stack.push((priority(&root), std::cmp::Reverse(0usize), root.clone()));
    'walk: while let Some((_, std::cmp::Reverse(depth), dir)) = stack.pop() {
        if started.elapsed() >= Duration::from_secs(options.seconds) {
            report.complete = false;
            report.stop_reason = "达到扫描时限，可对大目录单独扫描".into();
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                record_error(&mut report, &dir, &e);
                continue;
            }
        };
        for item in entries {
            if report.entries_visited >= options.max_entries
                || started.elapsed() >= Duration::from_secs(options.seconds)
            {
                report.complete = false;
                report.stop_reason = "达到时间或条目上限，可缩小范围继续扫描".into();
                break 'walk;
            }
            report.entries_visited += 1;
            let entry = match item {
                Ok(e) => e,
                Err(e) => {
                    record_error(&mut report, &dir, &e);
                    continue;
                }
            };
            let path = entry.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    record_error(&mut report, &path, &e);
                    continue;
                }
            };
            if skip(&meta) {
                report.skipped_reparse_or_cloud += 1;
                continue;
            }
            if meta.is_dir() {
                stack.push((priority(&path), std::cmp::Reverse(depth + 1), path));
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let bytes = meta.len();
            report.files += 1;
            report.logical_bytes = report.logical_bytes.saturating_add(bytes);
            for ancestor in path
                .ancestors()
                .skip(1)
                .take_while(|p| p.starts_with(&root))
            {
                let t = totals.entry(ancestor.to_path_buf()).or_default();
                t.0 = t.0.saturating_add(bytes);
                t.1 += 1;
            }
            report.largest_files.push(Entry {
                path: path.display().to_string(),
                bytes,
                files: 1,
                category: category(&path),
            });
            if report.largest_files.len() > 60 {
                report
                    .largest_files
                    .sort_by_key(|e| std::cmp::Reverse(e.bytes));
                report.largest_files.truncate(30);
            }
        }
    }
    report
        .largest_files
        .sort_by_key(|e| std::cmp::Reverse(e.bytes));
    report.largest_files.truncate(30);
    // Strategies need all measured directories, before the display list is truncated.
    let mut directories: Vec<Entry> = totals
        .into_iter()
        .map(|(p, (bytes, files))| Entry {
            path: p.display().to_string(),
            bytes,
            files,
            category: category(&p),
        })
        .collect();
    directories.sort_by_key(|e| std::cmp::Reverse(e.bytes));
    if report.unreadable_entries > 0 {
        report.complete = false;
        if report.stop_reason == "遍历完成" {
            report.stop_reason = "遍历结束，存在不可读条目".into();
        }
    }
    report.strategies = plan::build(
        &directories,
        &report.drives,
        options.target.as_deref(),
        &root,
        report.complete,
    )?;
    report.directories = directories
        .into_iter()
        .filter(|e| e.path != report.root)
        .take(100)
        .collect();
    report.elapsed_seconds = started.elapsed().as_secs_f64();
    Ok(report)
}
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse(args)?;
    let report = scan(&options)?;
    if options.json {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!("Hostwatch · C 盘分析\n{}\n{} · {:.1}s · {} 个文件 · {:.2} GiB（逻辑长度）\n跳过云占位/联接 {} · 不可读 {}",report.root.escape_debug(),report.stop_reason,report.elapsed_seconds,report.files,report.logical_bytes as f64/1073741824.0,report.skipped_reparse_or_cloud,report.unreadable_entries);
        for e in report.directories.iter().take(20) {
            println!(
                "{:>8.2} GiB  {}  {}",
                e.bytes as f64 / 1073741824.0,
                e.category,
                e.path.escape_debug()
            );
        }
        println!("\n清理与迁移策略（父子目录有重叠，容量不能相加）：");
        for s in &report.strategies {
            println!(
                "\n[{}] {} · {:.2} GiB\n{}\n{}",
                s.risk,
                s.title,
                s.measured_bytes as f64 / 1073741824.0,
                s.source.escape_debug(),
                s.steps.join("\n")
            );
            if let Some(c) = &s.preview_command {
                println!("仅预演：{c}");
            }
        }
        for l in &report.limitations {
            println!("说明：{l}");
        }
    }
    Ok(())
}
