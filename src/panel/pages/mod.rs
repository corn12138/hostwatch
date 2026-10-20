//! One module per popover page, plus helpers several pages share.

mod detail;
mod home;
mod issues;
mod processes;
mod settings;

use super::*;
pub(crate) use detail::{cpu_detail, disk_detail, memory_detail};
pub(crate) use home::home;
pub(crate) use issues::{handling, issues};
pub(crate) use processes::processes;
pub(crate) use settings::settings;

pub(crate) fn freshness_note(view: &View, sources: &str) -> String {
    match shown(view) {
        Some((latest, old)) => {
            let suffix = match view.health {
                Health::Paused if old => "（已暂停，非当前）",
                Health::Stale if old => "（已过时，非当前）",
                _ => "",
            };
            format!(
                "样本时间 {}{suffix} · 来源：{sources}",
                clock(latest.snapshot.unix_seconds)
            )
        }
        None => format!("暂无当前样本 · 来源：{sources}"),
    }
}

pub(crate) fn tool_feedback(view: &View, tool: Tool, blocks: &mut Vec<Block>) {
    let Some(feedback) = view.monitor.tool_feedback().filter(|f| f.tool == tool) else {
        return;
    };
    blocks.push(match &feedback.result {
        Ok(()) => Block::Card {
            tone: Tone::Neutral,
            title: format!("已请求打开{}", tool.label()),
            body: vec![format!(
                "{} 已交给系统打开。如果没有出现，可以手动打开：{}。",
                clock(feedback.at),
                tool.manual_path()
            )],
            buttons: Vec::new(),
        },
        Err(error) => Block::Card {
            tone: Tone::Critical,
            title: format!("无法打开{}", tool.label()),
            body: vec![
                error.clone(),
                format!("可以手动打开：{}。", tool.manual_path()),
            ],
            buttons: vec![Button::new("重试", Command::Open(tool))],
        },
    });
}

pub(crate) fn related_issue(view: &View, group: Group, buttons: &mut Vec<Button>) {
    if issue_for(view, group).is_some() {
        buttons.insert(
            0,
            Button::new("查看处理方法", Command::Go(Route::Handling(group))).primary(),
        );
    }
}
