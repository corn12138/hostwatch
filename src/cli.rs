#[derive(Debug, PartialEq)]
pub struct Options {
    pub watch: bool,
    /// Native menu-bar app (macOS); only `--interval` applies.
    pub menubar: bool,
    pub json: bool,
    pub processes: bool,
    pub interval: u64,
    pub count: Option<u64>,
}

pub const HELP: &str = "hostwatch — macOS 只读资源诊断（Windows 预留）

用法：
  hostwatch [snapshot] [--json] [--processes]
  hostwatch watch [--interval 秒] [--count 次数] [--json] [--processes]
  hostwatch menubar [--interval 秒]

默认每 30 秒采集；间隔范围 5–3600 秒；Ctrl+C 停止。
--processes  附带可读取进程的 RSS 前五名（额外采集成本）
--json       输出 JSON；watch 每行一个完整报告（NDJSON）
--count      有限采样次数，正整数；省略则持续运行
menubar      macOS 菜单栏常驻（无 Dock 图标）；面板内可改间隔、暂停、退出
             从 Hostwatch.app 无参数启动时等同于 menubar

本工具不会自动清理文件、终止进程或安装后台服务。";

pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        watch: false,
        menubar: false,
        json: false,
        processes: false,
        interval: 30,
        count: None,
    };
    let mut iter = args.iter().peekable();
    if let Some(command) = iter.peek() {
        match command.as_str() {
            "watch" => {
                options.watch = true;
                iter.next();
            }
            "menubar" => {
                options.menubar = true;
                iter.next();
            }
            "snapshot" => {
                iter.next();
            }
            _ => {}
        }
    }
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" if !options.menubar => options.json = true,
            "--processes" if !options.menubar => options.processes = true,
            "--interval" if options.menubar || options.watch => {
                let value = parse_number(arg, iter.next())?;
                if !(5..=3600).contains(&value) {
                    return Err("采样间隔必须在 5–3600 秒之间".into());
                }
                options.interval = value;
            }
            "--count" if options.watch => {
                let value = parse_number(arg, iter.next())?;
                if value == 0 {
                    return Err("采样次数必须大于 0".into());
                }
                options.count = Some(value);
            }
            _ => {
                return Err(format!(
                    "未知参数或不适用于当前命令：{arg}；使用 --help 查看用法"
                ))
            }
        }
    }
    Ok(options)
}

pub fn inside_app_bundle(executable: &str) -> bool {
    executable.contains(".app/Contents/MacOS/")
}

fn parse_number(arg: &str, value: Option<&String>) -> Result<u64, String> {
    value
        .ok_or_else(|| format!("{arg} 缺少数值"))?
        .parse::<u64>()
        .map_err(|_| format!("{arg} 需要正整数"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rejects_busy_loop_and_accidental_unbounded_count() {
        for values in [
            vec!["watch", "--interval", "0"],
            vec!["watch", "--count", "0"],
            vec!["watch", "--count", "no"],
            vec!["watch", "--interval"],
            vec!["snapshot", "--count", "2"],
            vec!["menubar", "--json"],
            vec!["menubar", "--count", "2"],
            vec!["menubar", "--interval", "4"],
        ] {
            assert!(parse(&args(&values)).is_err());
        }
    }

    #[test]
    fn bounded_watch_and_low_overhead_defaults() {
        let options = parse(&args(&[
            "watch",
            "--interval",
            "5",
            "--count",
            "3",
            "--json",
        ]))
        .unwrap();
        assert_eq!(options.count, Some(3));
        assert_eq!(options.interval, 5);
        assert!(options.json && options.watch && !options.processes);
        assert_eq!(parse(&[]).unwrap().interval, 30);
        assert!(inside_app_bundle(
            "/Applications/Hostwatch.app/Contents/MacOS/hostwatch"
        ));
        assert!(!inside_app_bundle("/usr/local/bin/hostwatch"));
        let menubar = parse(&args(&["menubar", "--interval", "10"])).unwrap();
        assert!(menubar.menubar && !menubar.watch && menubar.interval == 10);
    }
}
