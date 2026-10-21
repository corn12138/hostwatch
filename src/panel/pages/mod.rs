//! One module per popover page, plus helpers several pages share.

mod detail;
mod home;
mod issues;
mod optimize;
mod processes;
mod settings;

use super::*;
pub(crate) use detail::{cpu_detail, disk_detail, memory_detail, thermal_detail};
pub(crate) use home::home;
pub(crate) use issues::{handling, issues};
pub(crate) use optimize::optimize;
pub(crate) use processes::processes;
pub(crate) use settings::settings;

pub(crate) fn freshness_note(view: &View, sources: &str) -> String {
    let lang = view.lang;
    match shown(view) {
        Some((latest, old)) => {
            let at = clock(latest.snapshot.unix_seconds);
            let suffix = match view.health {
                Health::Paused if old => tr!(lang, "（已暂停，非当前）", " (paused, not current)"),
                Health::Stale if old => {
                    tr!(lang, "（已过时，非当前）", " (out of date, not current)")
                }
                _ => "",
            };
            tr!(
                lang,
                format!("样本时间 {at}{suffix} · 来源：{sources}"),
                format!("Sampled {at}{suffix} · Source: {sources}")
            )
        }
        None => tr!(
            lang,
            format!("暂无当前样本 · 来源：{sources}"),
            format!("No current sample · Source: {sources}")
        ),
    }
}

pub(crate) fn tool_feedback(view: &View, tool: Tool, blocks: &mut Vec<Block>) {
    let lang = view.lang;
    let Some(feedback) = view.monitor.tool_feedback().filter(|f| f.tool == tool) else {
        return;
    };
    let (name, path) = (tool_label(lang, tool), tool_path(lang, tool));
    blocks.push(match &feedback.result {
        Ok(()) => {
            let at = clock(feedback.at);
            Block::Card {
                tone: Tone::Neutral,
                title: tr!(
                    lang,
                    format!("已请求打开{name}"),
                    format!("Asked to open {name}")
                ),
                body: vec![tr!(
                    lang,
                    format!("{at} 已交给系统打开。如果没有出现，可以手动打开：{path}。"),
                    format!(
                        "Handed to macOS at {at}. If it does not appear, open it yourself: {path}."
                    )
                )],
                buttons: Vec::new(),
            }
        }
        Err(error) => Block::Card {
            tone: Tone::Critical,
            title: tr!(
                lang,
                format!("无法打开{name}"),
                format!("Could not open {name}")
            ),
            body: vec![
                error.clone(),
                tr!(
                    lang,
                    format!("可以手动打开：{path}。"),
                    format!("Open it yourself: {path}.")
                ),
            ],
            buttons: vec![Button::new(
                tr!(lang, "重试", "Try again"),
                Command::Open(tool),
            )],
        },
    });
}

pub(crate) fn related_issue(view: &View, group: Group, buttons: &mut Vec<Button>) {
    if issue_for(view, group).is_some() {
        buttons.insert(
            0,
            Button::new(
                tr!(view.lang, "查看处理方法", "What to do"),
                Command::Go(Route::Handling(group)),
            )
            .primary(),
        );
    }
}

/// "Open <tool>" button text.
pub(crate) fn open_tool_title(lang: Lang, tool: Tool) -> String {
    let name = tool_label(lang, tool);
    tr!(lang, format!("打开{name}"), format!("Open {name}"))
}
