# Hostwatch

轻量电脑资源诊断工具，**Rust 实现，macOS 优先，Windows 预留**。包含命令行工具和原生菜单栏面板（Hostwatch.app）。

这里的「运存 / 内存」均指 RAM；「磁盘空间」指持久存储。容量占满、内存压力和磁盘读写繁忙是不同问题，需要分开判断。

## 现在能做什么

- 原生采集物理内存分项（应用 / 联动 / 压缩 / 缓存文件 / 空闲）、swap 容量、内核内存压力、CPU 总体占用。
- 按两次样本差分计算交换换入/换出、页面读入速率；识别首个样本、计数器重置、睡眠或时钟跳变，这些情况下不给速率。
- 每个数据源逐项记录成功/失败、耗时与错误；失败的指标本次为空，不沿用旧值、不当作正常。
- 查看启动数据卷容量和可用空间，避免 APFS 共享卷重复汇总。
- 可选显示可读取进程的前五名（直接读内核 libproc 计数；同时给出 RSS 与活动监视器“内存”列同口径的物理内存占用）。
- 温度：macOS 公开的散热等级（正常 / 偏热 / 偏高 / 严重，偏高起系统会主动降速）与芯片、电池温度（传感器为非公开接口，读不到时只显示等级）。
- `watch`：有界历史（最近 120 样本）与持续异常告警——警告需连续 2 个有效样本，严重立即；连续 2 个正常样本才算恢复；同一规则 10 分钟冷却防抖动，升级到更高级别时立即提示。
- 规则：内存压力、交换频繁读写、CPU 饱和、磁盘空间不足；每条附证据和手动处理建议。
- 菜单栏面板（`menubar` / Hostwatch.app）：原生 AppKit `NSStatusItem` + `NSPopover`，单进程，无 WebView、无 Dock 图标、无辅助进程。首页一句结论（如「目前无需处理」）、内存 / 处理器 / 磁盘空间 / 温度四行和必要时一条处理建议；可进入各项详情（真实时间戳的趋势图，缺口如实留白）、处理方法、多项问题列表、按需进程排行、设置与数据说明（5 / 10 / 30 / 60 / 300 秒及启动参数自定义间隔、暂停 / 继续、退出）。恢复由告警引擎按样本自动判定，与是否点击按钮无关。面板打开或关闭时采样都继续；关闭只停止绘制。
- 一键优化：内存、交换、处理器或过热告警的卡片上提供「一键优化」。它在后台分析你打开的普通应用（含它们启动的辅助进程）谁占内存或处理器最多，默认勾选最重且不在前台的至多 3 个，确认后像按 ⌘Q 一样请它们正常退出：应用会先询问是否保存，可以取消；不强制结束、不删除文件、不改系统设置，访达、程序坞等系统组件与 Hostwatch 自身不在列表中。之后回报每个应用是否已退出；是否恢复仍由之后的样本判断。磁盘空间告警不提供自动操作。
- 与系统对照：内存口径与活动监视器相同（已用 = 应用 + 联动 + 压缩，按 1024 进制）；磁盘按 10 亿字节为 1 GB，与「储存空间」设置一样包含系统可自动释放的空间；进程排行与活动监视器“内存”列同口径；处理器为两次采样之间的平均值，会与活动监视器的短间隔读数有起伏差异。

目前没有系统通知、目录体积扫描、磁盘 I/O 诊断，不会自动删除文件或强制结束进程，也不开机自启；应用未经 Apple 公证。Windows 入口会明确返回「尚未实现」。

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

从 [Releases](https://github.com/corn12138/hostwatch/releases) 下载 `Hostwatch-<版本>-macos-arm64.zip`（Apple 芯片，macOS 13 及以上），解压后把 `Hostwatch.app` 拖到「应用程序」并打开。应用只做了临时（ad-hoc）签名、未经公证，首次打开会被系统拦截：到「系统设置 › 隐私与安全性」点「仍要打开」，或在终端执行 `xattr -dr com.apple.quarantine /Applications/Hostwatch.app`。不会开机自启；需要时手动打开。

### 本地更新与发布

```sh
./scripts/install.sh          # 构建 → 替换 /Applications/Hostwatch.app → 重新打开（本机构建无需 Gatekeeper 确认）
./scripts/release.sh 0.1.1    # 改版本号 → 检查 → 提交并推送 v0.1.1 标签 → GitHub Actions 自动打包并发布
```

`./scripts/package-app.sh` 只打包到 `dist/`。GitHub Actions：每次推送 `main` / PR 运行格式、clippy、测试和打包（`.github/workflows/ci.yml`）；推送 `v*` 标签时校验版本号并把 zip 附到 Release（`release.yml`）。应用本身不联网，不会自动检查更新。

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
- 告警阈值为产品启发式，不是 Apple 官方阈值：交换读写（换入+换出）≥ 1 MiB/s 警告、≥ 16 MiB/s 严重；CPU 总体占用 ≥ 90% 警告；磁盘可用（含可自动释放空间）低于 5 GB **或** 5% 为严重，否则低于 20 GB **或** 15% 为提醒；macOS 散热等级「偏高」为警告、「严重」为严重，温度数字本身不触发提示。
- 采集失败或速率不可用的样本对告警是「未知」：既不累计异常，也不算恢复，也不打断连续计数。
- 磁盘按启动 Data 卷读取，不汇总其他 APFS 卷。「可用」= statfs 可立即写入的空间 + 系统可自动释放的空间（后者向系统查询较慢，在后台每 10 分钟更新一次）。不表示某目录占用，也不检测 I/O 吞吐/延迟或硬盘健康。JSON 中 `available_bytes` 仍是 statfs 值，新增可选的 `important_available_bytes`。
- 进程排行仅限当前用户可读取的进程，可能漏掉受权限限制的进程；不推断泄漏。占用高不证明该进程就是卡顿原因。CLI 仍按 RSS 排序（兼容旧输出），JSON 新增可选的 `footprint_bytes`；面板按物理内存占用排序。
- 新增的 JSON 字段（`thermal`、`important_available_bytes`、`footprint_bytes`）都是可选附加字段，`schema_version` 仍为 2；`advice` 可能出现新的 `thermal_state` 规则。
- 只读取指标并输出到终端，不上传、不持久保存、不扫描文件内容；唯一会改变系统状态的操作是你在面板中确认的「请所选应用正常退出」。主动开启进程排行时输出包含应用名称与 PID，分享 JSON 前注意这部分信息。

## 开发与验证

```sh
./scripts/cargo.sh fmt --check
./scripts/cargo.sh test --locked
./scripts/cargo.sh clippy --locked --all-targets -- -D warnings
./scripts/cargo.sh build --locked --release
```

结构（每个文件只做一件事，均在约 400 行以内，测试单独成文件）：

```text
src/platform/      采集与平台隔离（macOS 实现，Windows 占位）
src/model.rs       数据模型（CLI JSON schema）
src/activity.rs    计数器差分、睡眠 / 重置判定
src/advice.rs      单样本规则
src/alerts.rs      持续告警状态机（Engine）与 120 样本历史
src/cli.rs main.rs 命令行与终端输出
src/monitor/       面板状态：mod（样本、暂停、间隔、过时、按需进程、工具反馈）、issues（问题分组与引擎恢复进度）
src/panel/         页面数据（与平台无关、可离线测试）：mod（类型与路由）、format、reading、chart、summary、pages/（每页一个文件）
src/menubar/       AppKit 外壳：controller（生命周期、定时器、弹出面板）、commands（各控件动作）、render（页面 → 视图树）、
                   kit / blocks / rows（可复用的小型原生视图套件）、views（自定义 NSView）、draw（原生绘图）
```

新增页面：在 `panel/pages/` 加一个文件生成 `Block`；只有新的 `Block` 种类才需要在 `menubar/blocks.rs` 加布局。

- [技术选型](docs/architecture.md)：Rust / Go 比较、数据口径、资源预算。
- [阶段状态与下一阶段任务](docs/roadmap.md)：菜单栏、通知、诊断深度和 Windows 槽位。
- [本机验证记录](docs/validation.md)：实测与未验证边界。

## 许可证

[MIT](LICENSE)。
