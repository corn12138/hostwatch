#[derive(Debug, PartialEq)]
pub struct Options {
    pub watch: bool,
    pub json: bool,
    pub processes: bool,
    pub interval: u64,
    pub count: Option<u64>,
}

pub const HELP: &str = "hostwatch — macOS 只读资源诊断（Windows 预留）

用法：
  hostwatch [snapshot] [--json] [--processes]
  hostwatch watch [--interval 秒] [--count 次数] [--json] [--processes]

默认每 30 秒采集；间隔范围 5–3600 秒；Ctrl+C 停止。
--processes  附带可读取进程的 RSS 前五名（额外采集成本）
--json       输出 JSON；watch 每行一个完整报告（NDJSON）
--count      有限采样次数，正整数；省略则持续运行

本工具不会自动清理文件、终止进程或安装后台服务。";

pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        watch: false,
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
            "snapshot" => {
                iter.next();
            }
            _ => {}
        }
    }
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => options.json = true,
            "--processes" => options.processes = true,
            "--interval" | "--count" if options.watch => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("{arg} 缺少数值"))?
                    .parse::<u64>()
                    .map_err(|_| format!("{arg} 需要正整数"))?;
                if arg == "--interval" {
                    if !(5..=3600).contains(&value) {
                        return Err("采样间隔必须在 5–3600 秒之间".into());
                    }
                    options.interval = value;
                } else {
                    if value == 0 {
                        return Err("采样次数必须大于 0".into());
                    }
                    options.count = Some(value);
                }
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
    }
}
