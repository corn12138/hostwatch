use super::*;
use crate::space::{Kind, SpaceState, Stop};
use std::path::Path;

/// Rows shown per folder; the rest are summed into the total.
const SHOWN: usize = 8;

/// `~/Library/Caches` style, so home paths stay short and private details stay out of view.
fn short(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn disk_size(bytes: u64) -> String {
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.0} MB", bytes as f64 / 1e6)
    }
}

pub(crate) fn space(view: &View, blocks: &mut Vec<Block>) {
    let lang = view.lang;
    let space = view.monitor.space();
    let home = space.home();
    let home_key = home.display().to_string();
    blocks.push(Block::Text(
        tr!(
            lang,
            "按需统计个人文件夹中哪些文件夹占用最多空间。只读取文件信息，不打开、不移动、不删除任何文件；删除与否由你在访达中决定。",
            "On request, finds which folders in your home folder take the most space. It only reads file information and never opens, moves or deletes anything; what to delete is your call in Finder."
        )
        .into(),
    ));
    match space.state() {
        None => blocks.push(Block::Buttons(vec![Button::new(
            tr!(lang, "统计个人文件夹", "Scan home folder"),
            Command::ScanFolder(home_key),
        )
        .primary()])),
        Some(SpaceState::Scanning { root, items, .. }) => {
            let root = short(home, root);
            blocks.push(Block::Note(tr!(
                lang,
                format!(
                    "正在统计 {root}…已查看 {items} 项（最多 {} 秒）",
                    crate::space::BUDGET.as_secs()
                ),
                format!(
                    "Scanning {root}… {items} items so far (at most {} seconds)",
                    crate::space::BUDGET.as_secs()
                )
            )));
            blocks.push(Block::Buttons(vec![Button::new(
                tr!(lang, "停止", "Stop"),
                Command::CancelSpace,
            )]));
        }
        Some(SpaceState::Failed { root, at, error }) => blocks.push(Block::Card {
            tone: Tone::Critical,
            title: tr!(
                lang,
                format!("无法统计 {}", short(home, root)),
                format!("Could not scan {}", short(home, root))
            ),
            body: vec![tr!(
                lang,
                format!("{} 时：{error}", clock(*at)),
                format!("At {}: {error}", clock(*at))
            )],
            buttons: vec![Button::new(
                tr!(lang, "统计个人文件夹", "Scan home folder"),
                Command::ScanFolder(home_key),
            )],
        }),
        Some(SpaceState::Done { at, report }) => {
            let root = short(home, &report.root);
            let lower = report.stopped.is_some() || report.entries.iter().any(|e| e.partial);
            let total = disk_size(report.total);
            blocks.push(Block::Big {
                value: if lower {
                    tr!(lang, format!("至少 {total}"), format!("At least {total}"))
                } else {
                    total
                },
                unit: tr!(
                    lang,
                    format!("{root}（{} 统计）", clock(*at)),
                    format!("{root} (scanned {})", clock(*at))
                ),
            });
            let largest = report.entries.first().map_or(1, |e| e.bytes.max(1));
            let rows: Vec<ProcessRow> = report
                .entries
                .iter()
                .take(SHOWN)
                .enumerate()
                .map(|(i, e)| {
                    let path = e.path.display().to_string();
                    let (name, size, command) = match e.kind {
                        Kind::Folder => (
                            format!("{} ›", e.name),
                            if e.partial {
                                tr!(
                                    lang,
                                    format!("≥ {}", disk_size(e.bytes)),
                                    format!("≥ {}", disk_size(e.bytes))
                                )
                            } else {
                                disk_size(e.bytes)
                            },
                            Some(Command::ScanFolder(path)),
                        ),
                        Kind::Protected => (
                            format!("{} ›", e.name),
                            tr!(lang, "点按统计", "Tap to scan").into(),
                            Some(Command::ScanFolder(path)),
                        ),
                        Kind::Unreadable => (
                            e.name.clone(),
                            tr!(lang, "无法读取", "Unreadable").into(),
                            None,
                        ),
                        Kind::File => (e.name.clone(), disk_size(e.bytes), None),
                    };
                    ProcessRow {
                        rank: i + 1,
                        name,
                        size,
                        fraction: e.bytes as f64 / largest as f64,
                        command,
                    }
                })
                .collect();
            if rows.is_empty() {
                blocks.push(Block::Text(
                    tr!(lang, "这个文件夹是空的。", "This folder is empty.").into(),
                ));
            } else {
                blocks.push(Block::Processes(rows));
            }
            let mut notes: Vec<String> = Vec::new();
            match report.stopped {
                Some(Stop::Budget) => notes.push(tr!(
                    lang,
                    format!("统计到 {} 秒上限时停止，{} 个文件夹未统计完，标“≥”的是至少值。点进较大的文件夹可以单独统计。", crate::space::BUDGET.as_secs(), report.unfinished),
                    format!("Stopped at the {}-second limit with {} folder(s) unfinished; sizes marked “≥” are lower bounds. Open a large folder to scan it on its own.", crate::space::BUDGET.as_secs(), report.unfinished)
                )),
                Some(Stop::Cancelled) => notes.push(tr!(
                    lang,
                    format!("已停止，{} 个文件夹未统计完，标“≥”的是至少值。", report.unfinished),
                    format!("Stopped with {} folder(s) unfinished; sizes marked “≥” are lower bounds.", report.unfinished)
                )),
                None => {}
            }
            if report.entries.iter().any(|e| e.kind == Kind::Protected) || report.guarded > 0 {
                notes.push(tr!(
                    lang,
                    "桌面、文稿、下载、iCloud 云盘和其他应用的数据受 macOS 保护，默认不统计；点按它们才会统计，macOS 可能会先询问是否允许。",
                    "Desktop, Documents, Downloads, iCloud Drive and other apps' data are protected by macOS and not scanned by default; tap one to scan it, and macOS may ask first."
                ).into());
            }
            if report.unreadable > 0 {
                let n = report.unreadable;
                notes.push(tr!(
                    lang,
                    format!("{n} 个文件夹读不到（通常是系统或其他应用的数据），未计入。"),
                    format!("{n} folder(s) could not be read (usually system or other apps' data) and are not counted.")
                ));
            }
            notes.push(tr!(
                lang,
                "按实际占用的磁盘空间计算，1 GB = 10 亿字节；同一文件的多个硬链接只算一次，不跟随替身，不包括其他磁盘。APFS 克隆和快照会让合计与“储存空间”设置不同。",
                "Counts space actually used on disk, with 1 GB = 1,000,000,000 bytes; hard links count once, aliases are not followed, other disks are excluded. APFS clones and snapshots can make totals differ from Storage settings."
            ).into());
            for note in notes {
                blocks.push(Block::Note(note));
            }
            let mut buttons = Vec::new();
            if report.root != home {
                if let Some(parent) = report.root.parent().filter(|p| p.starts_with(home)) {
                    buttons.push(Button::new(
                        tr!(lang, "上一层", "Up one level"),
                        Command::ScanFolder(parent.display().to_string()),
                    ));
                }
            }
            buttons.push(Button::new(
                tr!(lang, "在访达中显示", "Show in Finder"),
                Command::Reveal(report.root.display().to_string()),
            ));
            buttons.push(Button::new(
                tr!(lang, "重新统计", "Scan again"),
                Command::ScanFolder(report.root.display().to_string()),
            ));
            blocks.push(Block::Buttons(buttons));
            tool_feedback(view, Tool::Finder, blocks);
        }
    }
    blocks.push(Block::Buttons(vec![Button::new(
        open_tool_title(lang, Tool::StorageSettings),
        Command::Open(Tool::StorageSettings),
    )]));
    tool_feedback(view, Tool::StorageSettings, blocks);
}
