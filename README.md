# Hostwatch

轻量电脑资源诊断工具，**Rust 实现，macOS 优先，Windows 预留**。包含命令行工具和原生菜单栏面板（Hostwatch.app）。

这里的「运存 / 内存」均指 RAM；「磁盘空间」指持久存储。容量占满、内存压力和磁盘读写繁忙是不同问题，需要分开判断。

## 现在能做什么

- 原生采集物理内存分项（应用 / 联动 / 压缩 / 缓存文件 / 空闲）、swap 容量、内核内存压力、CPU 总体占用。
- 按两次样本差分计算交换换入/换出、页面读入速率；识别首个样本、计数器重置、睡眠或时钟跳变，这些情况下不给速率。
- 每个数据源逐项记录成功/失败、耗时与错误；失败的指标本次为空，不沿用旧值、不当作正常。
- 查看启动数据卷容量和可用空间，避免 APFS 共享卷重复汇总。
- 可选显示可读取进程的 RSS 前五名，辅助定位应用。
- `watch`：有界历史（最近 120 样本）与持续异常告警——警告需连续 2 个有效样本，严重立即；连续 2 个正常样本才算恢复；同一规则 10 分钟冷却防抖动，升级到更高级别时立即提示。
- 规则：内存压力、交换频繁读写、CPU 饱和、磁盘空间不足；每条附证据和手动处理建议。
- 菜单栏面板（`menubar` / Hostwatch.app）：原生 AppKit `NSStatusItem` + `NSPopover`，单进程，无 WebView、无 Dock 图标、无辅助进程。首页一句结论（如「目前无需处理」）、内存 / 处理器 / 磁盘空间三行和必要时一条处理建议；可进入三项资源详情（真实时间戳的趋势图，缺口如实留白）、处理方法（原因、手动步骤、打开活动监视器或“储存空间”设置及失败时的手动路径）、多项问题列表、按需进程内存排行、设置与数据说明（5 / 10 / 30 / 60 / 300 秒及启动参数自定义间隔、暂停 / 继续、退出）。恢复由告警引擎按样本自动判定，与是否点击按钮无关。面板打开或关闭时采样都继续；关闭只停止绘制。

目前没有系统通知、目录体积扫描、磁盘 I/O 诊断，也没有自动清理或开机自启；应用未经 Apple 公证。Windows 入口会明确返回「尚未实现」。

## 运行

本地已用 Rust 1.79 验证；依赖版本由 `Cargo.lock` 固定。实测默认 watch 峰值 RSS 约 5.6 MiB、CPU 远低于 1%；菜单栏面板在本机（macOS 27.2，arm64，5 秒间隔）关闭时约 71–73 MiB RSS / 13.8 MB 物理内存足迹，打开时约 102 MiB / 33 MB，反复开合 200 次后约 118 MiB / 65 MB；均超出暂定的 40 MiB RSS 预算，改用足迹预算仍是待确认提议，详见 [validation.md](docs/validation.md)。

```sh
./scripts/cargo.sh run --locked --release -- snapshot
./scripts/cargo.sh run --locked --release -- snapshot --processes
./scripts/cargo.sh run --locked --release -- watch
./scripts/cargo.sh run --locked --release -- watch --interval 10 --count 3 --json
./scripts/cargo.sh run --locked --release -- menubar --interval 30
```

菜单栏模式常驻菜单栏：点图标打开 / 关闭面板，点面板外或按 Esc 关闭（子页面中 Esc 先返回上一层）；「更多 › 退出 Hostwatch」结束整个进程，不留后台采集。暂停会停掉采样定时器，继续时重新取基线样本。

### 安装 Hostwatch.app

从 [Releases](https://github.com/corn12138/hostwatch/releases) 下载 `Hostwatch-<版本>-macos-arm64.zip`（Apple 芯片，macOS 13 及以上），解压后把 `Hostwatch.app` 拖到「应用程序」并打开。应用只做了临时（ad-hoc）签名、未经公证，首次打开会被系统拦截：到「系统设置 › 隐私与安全性」点「仍要打开」，或在终端执行 `xattr -dr com.apple.quarantine /Applications/Hostwatch.app`。不会开机自启；需要时手动打开。本地自行打包：`./scripts/package-app.sh`（输出到 `dist/`）。

`Ctrl+C` 停止。采样间隔为 5–3600 秒；进程排行默认关闭以降低采集成本。`snapshot` 会先取一个基线样本、等待 1 秒再输出，以便给出 CPU 与交换速率。终端运行期间才会监测，退出后不会继续后台执行。

当前目录名含 `:`，Cargo 在 macOS 拼接动态库路径时不能使用包含冒号的构建输出目录。脚本仅针对这种路径把产物放到 `$HOME/Library/Caches/hostwatch/build-<路径校验值>`；不改目录名和全局 Rust 配置。可用以下命令确认产物目录：

```sh
./scripts/cargo.sh metadata --no-deps --format-version 1
```

其他普通路径中也可直接运行 `cargo run --locked --release`。

## 指标与建议边界

- 单位统一为 bytes；终端以 GiB / MiB 显示，JSON 用原始字节和 bytes/s。JSON 为 `schema_version: 2`，包含 Unix 秒时间戳、`activity.status`（`ready` / `warmup` / `counter_reset` / `sleep_gap` / `unavailable`）和 `sources` 逐项状态；只有 `watch` 输出 `events`。
- 内存分项由 `host_statistics64` 页计数推导：已用 = 应用（内部页 − 可清除页）+ 联动 + 压缩；缓存文件 = 文件页 + 可清除页。这是接近活动监视器的估计口径，不保证逐项一致。
- 内核压力来自 `kern.memorystatus_vm_pressure_level`，并非活动监视器曲线的复刻。该接口不是稳定公开 API；不可用/未知值时明确显示未知。
- RAM 使用率高或 swap 存量高，不单独触发清理建议。正常缓存能提升性能；swap 存量不等于正在频繁交换，所以单独看交换**速率**。
- 速率使用不含睡眠的单调时钟；墙钟比单调时钟多走超过 5 秒（或 10%）即判定睡眠/时钟跳变，本样本不计算速率。CPU tick 是 32 位计数，单次回绕可恢复。
- 告警阈值为产品启发式，不是 Apple 官方阈值：交换读写（换入+换出）≥ 1 MiB/s 警告、≥ 16 MiB/s 严重；CPU 总体占用 ≥ 90% 警告；磁盘可用低于 5 GiB **或** 5% 为严重，否则低于 20 GiB **或** 15% 为提醒。
- 采集失败或速率不可用的样本对告警是「未知」：既不累计异常，也不算恢复，也不打断连续计数。
- 磁盘按启动 Data 卷读取，不汇总其他 APFS 卷。不表示某目录占用，也不检测 I/O 吞吐/延迟、硬盘健康或可清除空间。
- 进程排行仅限可读取的 RSS，可能漏掉受权限限制的进程；不合并应用多进程、不推断泄漏。高 RSS 不证明该进程就是卡顿原因。
- 只读取指标并输出到终端，不上传、不持久保存、不扫描文件内容。主动开启进程排行时输出包含应用名称与 PID，分享 JSON 前注意这部分信息。

## 开发与验证

```sh
./scripts/cargo.sh fmt --check
./scripts/cargo.sh test --locked
./scripts/cargo.sh clippy --locked --all-targets -- -D warnings
./scripts/cargo.sh build --locked --release
```

结构：`src/platform/` 负责采集与平台隔离，`src/model.rs` 定义数据，`src/activity.rs` 做计数器差分与睡眠/重置判定，`src/advice.rs` 是单样本规则，`src/alerts.rs` 是持续告警状态机与有界历史，`src/cli.rs` / `src/main.rs` 负责命令行与输出，`src/monitor.rs` 是与平台无关的面板状态（样本、问题分组、缺口证据、按需进程、工具反馈），`src/panel.rs` 把状态变成页面与图表数据（各状态可离线测试），`src/menubar.rs` 是 AppKit 外壳（状态栏项、弹出面板、原生绘图）。除采集外均可用固定样本离线测试。

- [技术选型](docs/architecture.md)：Rust / Go 比较、数据口径、资源预算。
- [阶段状态与下一阶段任务](docs/roadmap.md)：菜单栏、通知、诊断深度和 Windows 槽位。
- [本机验证记录](docs/validation.md)：实测与未验证边界。

## 许可证

[MIT](LICENSE)。
