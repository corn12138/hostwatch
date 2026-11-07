use super::Entry;
use crate::model::Disk;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Strategy {
    pub title: &'static str,
    pub risk: &'static str,
    pub source: String,
    pub measured_bytes: u64,
    pub destination: Option<String>,
    pub target_has_capacity: Option<bool>,
    pub steps: Vec<String>,
    pub preview_command: Option<String>,
}
fn clean(path: &str) -> String {
    path.strip_prefix("\\\\?\\")
        .unwrap_or(path)
        .replace('/', "\\")
}
// Commands are PowerShell source: literal single-quoted arguments, never interpolated strings.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn target_path(target: &Path, source: &Path) -> PathBuf {
    let source = clean(&source.to_string_lossy());
    // Preserve user/project hierarchy; two folders named Downloads must not share a target.
    target
        .join("Hostwatch-Migration")
        .join(source.get(3..).unwrap_or(&source))
}
pub fn build(
    entries: &[Entry],
    drives: &[Disk],
    requested: Option<&Path>,
    root: &Path,
    scan_complete: bool,
) -> Result<Vec<Strategy>, String> {
    let root_text = clean(&root.to_string_lossy());
    let source_drive = root_text.get(..2).unwrap_or("").to_uppercase();
    let requested = requested.map(|p| clean(&p.to_string_lossy()));
    if let Some(target) = &requested {
        let valid = target.len() >= 3
            && target.as_bytes()[0].is_ascii_alphabetic()
            && target.as_bytes().get(1..3) == Some(b":\\")
            && !target.contains(['\r', '\n']);
        if !valid || target[..2].eq_ignore_ascii_case(&source_drive) {
            return Err(
                "迁移目标必须是其他本地盘符的绝对路径，例如 D:\\；不能使用当前扫描盘".into(),
            );
        }
        if !drives
            .iter()
            .any(|d| d.path[..2].eq_ignore_ascii_case(&target[..2]))
        {
            return Err("目标盘不存在、不可读取或不是本地固定磁盘".into());
        }
    }
    let target = requested.map(PathBuf::from).or_else(|| {
        drives
            .iter()
            .filter(|d| !d.path[..2].eq_ignore_ascii_case(&source_drive))
            .max_by_key(|d| d.available_bytes)
            .map(|d| PathBuf::from(&d.path))
    });
    let mut result = Vec::new();
    for e in entries {
        let source = clean(&e.path);
        let lower = source.to_lowercase();
        let leaf = Path::new(&e.path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let (title, risk, steps, movable): (&'static str, &'static str, Vec<&str>, bool) = if [
            "downloads",
            "documents",
            "videos",
            "pictures",
            "music",
            "desktop",
        ]
        .contains(&leaf.as_str())
            && e.category == "个人文件"
        {
            ("迁移个人文件夹","低风险",vec!["先确认备份，暂停下载与编辑；检查文件夹是否由 OneDrive 管理。", "优先在资源管理器中右键文件夹 → 属性 → 位置 → 移动，选择其他磁盘的专用目录。OneDrive 管理的文件夹应先使用 OneDrive 自身设置。", "下方命令只列出待复制内容，不会移动文件。正式迁移后检查文件数量、抽样打开文件并核对重要文件哈希，更新应用保存位置后再考虑释放旧副本。"],true)
        } else if [
            "npm-cache",
            ".npm",
            ".nuget",
            ".cargo",
            "node_modules",
            ".venv",
        ]
        .contains(&leaf.as_str())
            || lower.ends_with("\\pip\\cache")
        {
            ("调整开发依赖与缓存位置","需检查",vec!["先关闭构建任务；node_modules / .venv 应在迁移项目后通过锁文件重新安装，不要直接跨路径搬运虚拟环境。", "npm：通过 npm config set cache 设置新位置；pip：通过 pip cache dir 确认后设置 PIP_CACHE_DIR；NuGet 使用 NUGET_PACKAGES；Cargo 使用 CARGO_HOME，迁移后检查工具链路径。", "仅按当前工具的官方配置迁移，并验证新项目安装和构建成功后再审查旧缓存。此容量包含可能需要保留的依赖，不保证全部可释放。"],false)
        } else if (leaf == "temp" || leaf == "cache" || leaf == "$recycle.bin")
            && e.category != "系统与应用"
        {
            ("审查临时文件与缓存","低风险",vec!["打开 Windows 设置 → 系统 → 存储 → 临时文件，逐项查看用途。下载目录、回收站与旧版 Windows 文件需单独确认。", "浏览器和应用缓存优先通过对应应用清理；关闭应用后再复查。这里的逻辑长度不是安全删除量，工具不会直接删除。"],false)
        } else if leaf == "windows"
            || leaf == "program files"
            || leaf == "program files (x86)"
            || leaf == "programdata"
        {
            ("系统与已安装应用保持原位","禁止直接搬移",vec!["不要手动移动 Windows、WinSxS、Installer、Program Files 或整个 ProgramData，也不要给这些目录建立跨盘联接。", "系统组件使用 Windows 存储清理或系统维护工具；应用在设置 → 应用 → 已安装的应用中使用可用的「移动」，否则卸载后重装到其他盘。", "Steam 库、Docker 数据与 WSL 虚拟磁盘使用各自的迁移/导出导入功能；pagefile.sys、hiberfil.sys 不可手动删除或搬移。"],false)
        } else if leaf == "docker" || leaf == "wsl" || lower.ends_with("\\packages") {
            ("通过应用管理大型数据","需检查",vec!["退出相关容器或发行版，备份数据。Docker 使用其磁盘镜像位置设置；WSL 使用受支持的导出/导入流程。", "虚拟磁盘与 Microsoft Store 应用数据有注册和权限信息，不要直接剪切 AppData / Packages 或 vhdx 文件。"],false)
        } else {
            continue;
        };
        if result.iter().any(|s: &Strategy| s.source == source) {
            continue;
        }
        let destination = if movable {
            target
                .as_ref()
                .map(|t| clean(&target_path(t, Path::new(&e.path)).to_string_lossy()))
        } else {
            None
        };
        let capacity = destination
            .as_ref()
            .and_then(|d| {
                drives
                    .iter()
                    .find(|v| v.path[..2].eq_ignore_ascii_case(&d[..2]))
            })
            .map(|d| {
                d.available_bytes >= e.bytes.saturating_add((e.bytes / 10).max(5 * 1073741824))
            });
        let fits_measured = capacity;
        // A bounded / unreadable scan is only a lower bound, so it cannot prove sufficient space.
        let capacity = if scan_complete || capacity == Some(false) {
            capacity
        } else {
            None
        };
        let preview = destination
            .as_ref()
            .filter(|_| fits_measured == Some(true))
            .map(|dest| {
                format!(
                    "& robocopy.exe {} {} /E /COPY:DAT /DCOPY:DAT /XJ /SL /R:0 /W:0 /L",
                    quote(&source),
                    quote(dest)
                )
            });
        let mut steps: Vec<String> = steps.into_iter().map(String::from).collect();
        if movable && !scan_complete {
            steps.push("扫描不完整，目标盘是否容得下全量文件仍未知；当前命令只能预演。请完整扫描源目录，并在正式迁移前重新检查空间。".into());
        }
        if movable && target.is_none() {
            steps.push("未发现其他可用固定磁盘；连接并检查目标磁盘后再生成迁移预演。".into());
        }
        if capacity == Some(false) {
            steps.push("目标盘空间不足：需容纳当前逻辑容量，另留至少 10% 或 5 GiB 余量。请更换目标盘或拆分迁移。".into());
        }
        result.push(Strategy {
            title,
            risk,
            source,
            measured_bytes: e.bytes,
            destination,
            target_has_capacity: capacity,
            steps,
            preview_command: preview,
        });
        if result.len() >= 40 {
            break;
        }
    }
    Ok(result)
}
