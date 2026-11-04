use super::*;
use crate::model::{AwakeKind, SleepBlocker};

/// Held longer than this, a blocker is worth a line on the overview.
pub(crate) const LONG_HOLD: u64 = 3600;

fn kind_label(lang: Lang, kind: AwakeKind) -> &'static str {
    match kind {
        AwakeKind::System => tr!(lang, "阻止电脑睡眠", "Keeps the Mac awake"),
        AwakeKind::Display => tr!(lang, "阻止屏幕关闭", "Keeps the display on"),
    }
}

fn long(blockers: &[SleepBlocker]) -> Vec<&SleepBlocker> {
    blockers
        .iter()
        .filter(|b| b.since_seconds.is_some_and(|s| s >= LONG_HOLD))
        .collect()
}

/// One line on the overview when something has held the Mac awake for over an hour.
pub(crate) fn awake_hint(view: &View, blocks: &mut Vec<Block>) {
    let lang = view.lang;
    let Some((_, Ok(blockers))) = view.monitor.awake() else {
        return;
    };
    let long = long(blockers);
    if long.is_empty() {
        return;
    }
    let mut names: Vec<String> = long
        .iter()
        .map(|b| display_name(lang, &b.process))
        .collect();
    names.dedup();
    let names = names.join(tr!(lang, "、", ", "));
    blocks.push(Block::Buttons(vec![Button::new(
        tr!(
            lang,
            format!("{names} 已阻止睡眠超过 1 小时 ›"),
            format!("{names} kept the Mac awake for over an hour ›")
        ),
        Command::Go(Route::Awake),
    )]));
}

pub(crate) fn awake(view: &View, blocks: &mut Vec<Block>) {
    let lang = view.lang;
    blocks.push(Block::Text(
        tr!(
            lang,
            "这些进程向 macOS 声明了“暂时不要睡眠”。播放视频或音乐、通话、下载、备份时这很正常；但没在做这些事时，它们会让电脑整夜不睡、电池耗得更快。",
            "These processes told macOS not to sleep for now. That is normal while playing video or music, on a call, downloading or backing up; when none of that is happening, they keep the Mac up all night and drain the battery."
        )
        .into(),
    ));
    match view.monitor.awake() {
        None => blocks.push(Block::Note(tr!(lang, "正在读取…", "Reading…").into())),
        Some((at, Err(error))) => blocks.push(Block::Card {
            tone: Tone::Unknown,
            title: tr!(lang, "读不到睡眠状态", "Could not read sleep state").into(),
            body: vec![tr!(
                lang,
                format!("{} 时：{error}", clock(*at)),
                format!("At {}: {error}", clock(*at))
            )],
            buttons: vec![Button::new(
                tr!(lang, "重新读取", "Read again"),
                Command::RefreshAwake,
            )],
        }),
        Some((at, Ok(blockers))) => {
            if blockers.is_empty() {
                blocks.push(Block::Big {
                    value: tr!(lang, "没有", "Nothing").into(),
                    unit: tr!(
                        lang,
                        "目前没有进程阻止睡眠",
                        "is keeping the Mac awake right now"
                    )
                    .into(),
                });
            } else {
                blocks.push(Block::Facts(
                    blockers
                        .iter()
                        .map(|b| {
                            let mut detail = kind_label(lang, b.kind).to_string();
                            if let Some(seconds) = b.since_seconds {
                                detail.push_str(&tr!(
                                    lang,
                                    format!(" · 已持续{}", span(lang, seconds)),
                                    format!(" · for {}", span(lang, seconds))
                                ));
                            }
                            // Many apps name the assertion after their framework ("Electron"),
                            // which says nothing; show only a name that adds something.
                            let reason = b.reason.trim();
                            if !reason.is_empty() && reason != b.process && reason != "Electron" {
                                detail.push_str(&tr!(
                                    lang,
                                    format!(" · “{reason}”"),
                                    format!(" · “{reason}”")
                                ));
                            }
                            (
                                format!("{} ({})", display_name(lang, &b.process), b.pid),
                                detail,
                            )
                        })
                        .collect(),
                ));
            }
            blocks.push(Block::Note(tr!(
                lang,
                format!("读取于 {}。Hostwatch 只列出，不会取消这些声明；结束对应的播放、通话或任务，或退出该应用后会自动消失。在终端运行 pmset -g assertions 可看到同样的信息。", clock(*at)),
                format!("Read at {}. Hostwatch only lists these and never cancels them; they go away when the playback, call or task ends or the app quits. Running pmset -g assertions in Terminal shows the same information.", clock(*at))
            )));
        }
    }
    blocks.push(Block::Buttons(vec![
        Button::new(tr!(lang, "重新读取", "Read again"), Command::RefreshAwake),
        Button::new(
            open_tool_title(lang, Tool::ActivityMonitor),
            Command::Open(Tool::ActivityMonitor),
        ),
    ]));
    tool_feedback(view, Tool::ActivityMonitor, blocks);
}
