# 本机验证记录

日期：2026-10-19。环境：macOS 27.2（26B5112a），Apple silicon（arm64），48 GiB 内存，16 KiB 内核页；Rust 1.79，`--release`（LTO、strip）。只代表这一台机器。

## 自动化检查

```sh
./scripts/cargo.sh fmt --check
./scripts/cargo.sh clippy --locked --all-targets -- -D warnings
./scripts/cargo.sh test --locked   # 弹出面板版本：47 单元 + 4 集成，全部通过
```

固定样本覆盖：首个样本 warmup、源不可用、计数器回退、睡眠（墙钟远超单调时钟）、墙钟倒退、CPU tick 32 位回绕；未知压力不算正常；warmup 时不宣称 CPU/交换健康；警告需 2 个连续样本、严重立即；未知样本不触发也不恢复；单个正常样本不恢复；冷却内抖动不重复提示、冷却后重新布防；升级立即提示；历史上限 120。菜单栏视图模型（`src/menu.rs`）另覆盖：加载中 → 预热 → 正常；持续告警带证据/建议与最近事件；采集失败不显示旧值且不清除告警；源失败/压力未知显示「部分数据不可用」而非正常；暂停标题与菜单项；间隔/命令编码往返；长文本换行与进程名转义；按需进程排行在后续常规样本中保留。

## 采集口径对照

同一时刻与系统命令对照（`vm_stat`、`sysctl vm.swapusage hw.memsize`）：

| 项 | hostwatch | 系统命令 | 结论 |
| --- | --- | --- | --- |
| 联动 | 4.25 GiB | wired 278492 页 × 16 KiB = 4.25 GiB | 一致 |
| 压缩 | 3.98 GiB | occupied by compressor 260695 页 = 3.98 GiB | 一致 |
| 缓存文件 | 13.15 GiB | file-backed 810450 + purgeable 51252 页 = 13.15 GiB | 一致 |
| 空闲 | 1.78 GiB | free 113753 + speculative 3468 页 ≈ 1.79 GiB | 一致（含 speculative） |
| swap | 0 / 0 | `vm.swapusage` total 0 | 一致；本机当时无交换文件 |

尚未与活动监视器界面逐项截图对照；「应用内存」用 内部页 − 可清除页 推导，与活动监视器的差异未量化。

## 资源占用

用 `/usr/bin/time -l` 测整个进程：

| 场景 | 时长 | 峰值 RSS | 峰值内存足迹 | CPU 时间（user+sys） |
| --- | --- | --- | --- | --- |
| `watch --interval 5 --count 36 --json`，无进程排行 | 175 s | 5.6 MiB | 2.0 MiB | 0.01 s（≈ 0.006% 单核） |
| `watch --interval 5 --count 12 --json --processes` | 55 s | 7.9 MiB | 4.1 MiB | 0.55 s（≈ 1% 单核，进程扫描为主） |

36 个样本中采集耗时（所有源合计）中位 135 µs、最大 260 µs；无源失败；首个样本 warmup，其余 35 个 ready；无告警事件（当时负载 CPU 约 35%、无交换）。

默认 30 秒间隔下成本按比例更低。进程排行在 5 秒间隔时接近 1% 预算，应保持默认关闭或在 UI 中按需刷新。

## 菜单栏 MVP（`hostwatch menubar`）

release 构建，`--interval 5`（比默认 30 秒更密，作为上限情形）。整个产品只有这一个进程：`pgrep` 运行时只见 `hostwatch menubar`，无 WebView 或辅助进程。证据分为五类，结论不可互相替代。

### 1. 单元测试（固定样本，`src/menu.rs`）

- **CPU 不可用却显示正常（审计 P2）**：`cpu_rate_missing_after_recovery_or_zero_delta_is_not_normal` 经生产代码 `activity::derive` 构造两类样本（上一样本无 CPU ticks / tick 零差分），数据源全部成功。修复前失败（`headline=("HW ✓", "未见持续异常")`，见 `docs/evidence/step3-2026-10-19/regression-before-fix.log`），修复后通过（`regression-after-fix.log`）。现在标题 `HW ?`、摘要「部分数据不可用：CPU 占用」；持续告警仍优先；下一个可测样本直接回到正常。规则：任何建议规则的观测为「未知」即不算正常（速率预热期仍报预热），告警引擎与冷却逻辑未改。
- **只读操作入口**：`advice_offers_matching_read_only_tools`（交换/内存/CPU → 打开活动监视器，磁盘 → 打开储存空间设置，健康摘要不附入口）；`failed_tool_open_stays_visible_until_a_success`（失败原因显示在菜单顶部直到下次成功，不改变标题、告警和样本）；tag 编码往返。
- 全部：30 单元 + 4 集成通过；`fmt --check`、`clippy -D warnings`、release 构建、`git diff --check` 通过。

### 2. 临时 harness（不在产品代码中；源码与输出存于 `docs/evidence/step3-2026-10-19/runloop-harness/`）

条件：独立二进制，主线程，`NSApplication` 已创建但未 `run()`；手动以 `NSEventTrackingRunLoopMode`（菜单跟踪使用的模式）运行主 run loop 2.1 s，每次 `runMode:beforeDate:` 外包 `autoreleasepool`；两个 0.2 s 重复定时器。

| 定时器注册方式 | 跟踪模式下触发次数 |
| --- | --- |
| `scheduledTimerWithTimeInterval…`（旧实现，仅默认模式） | 0 |
| `timerWithTimeInterval…` + `addTimer:forMode:NSRunLoopCommonModes`（新实现） | 10 |

同一 harness：`URLForApplicationWithBundleIdentifier("com.apple.ActivityMonitor")` 返回 `/System/Applications/Utilities/Activity Monitor.app`，伪造 bundle id 返回 nil（菜单走「未找到」错误分支）；`openURL` 对活动监视器与 `x-apple.systempreferences:com.apple.settings.Storage` 均返回 true，随后 `pgrep` 可见两个应用进程。**未能截图确认系统设置停在「储存空间」页**；也不是经菜单点击触发。

### 3. 真实 UI 检查：**被阻塞，未通过**

- `osascript`（System Events 读取菜单栏项）：`osascript is not allowed assistive access. (-1719)`。
- `screencapture -x`：`could not create image from display`（无屏幕录制权限）。
- 本会话没有可用的桌面控制工具。故总览 / 建议 / 诊断 / 排行 / 间隔勾选 / 暂停无采样 / 继续首样本预热 / 菜单中打开工具 / 菜单展开期间标题更新 / 菜单退出码 0，均**未做真实点击验收**。
- 本轮的 release 实例（PID 96395）在 30 分钟采样结束后被发现已退出，输出为空，退出原因与退出码未知；**不计作 Quit 验证**。

### 4. 资源（`scripts/proc-usage.py`，`proc_pid_rusage` v0）

可复跑：`hostwatch menubar --interval 5` 后执行 `scripts/proc-usage.py <PID> 1800 30`。单位：RSS 为 MiB（2^20 字节），物理内存足迹为 MB（10^6 字节），CPU 为 user+system 秒（Mach timebase 125/3 换算）。条件：正常 `NSApplication.run()` 事件循环（AppKit 每个事件排空自动释放池），采样回调自带 `autoreleasepool`；release 二进制 SHA-256 与系统信息见 `menubar-idle-30min.env`；原始数据 `menubar-idle-30min.tsv`。

| 阶段（2026-10-19，macOS 27.2，arm64，48 GiB） | RSS MiB | 足迹 MB | CPU |
| --- | --- | --- | --- |
| 0–150 s，菜单未打开 | 65.5–67.4 | 12.55–12.67 | 150 s 内 0.024 s |
| 约 180–210 s 出现阶跃（Claude 未操作菜单；GUI 未受控，形态与外部打开菜单一致） | → 96.0 | → 25.8 | 该 30 s 内 0.74 s |
| 210–1800 s | 96.0–96.6 | 25.8–26.5，无持续增长 | 约 1.05 s / 1590 s（≈ 0.07% 单核） |
| 全程 1800 s，61 个样本 | 65.5–96.6 | 12.55–26.46 | 2.14 s（平均 0.119% 单核） |

只能说明：本机、本版本、该场景下，菜单未打开时约 67 MiB RSS / 12.7 MB 足迹；出现一次未受控的跳升后在其余约 26 分钟内未见持续增长。**不能**说明长期无泄漏、8 小时行为、睡眠唤醒或菜单展开期间持续采样的 CPU 成本（菜单未能受控打开）。

### 5. 上一轮记录（Claude 自述，本仓库无可复跑钩子，审计未独立复验）

上一轮用临时自测钩子经 `perform:` 触发刷新排行、改间隔、暂停、继续、退出（报告退出码 0），并报告 12 000 次「重建菜单 + 采样」后足迹 14 MB、200 次开合后约 94 MiB RSS / 23 MB 足迹，以及「同机空 NSStatusItem 基线约 67 MiB RSS / 12 MB 足迹」。这些数字保留为参考，**不作为本轮验收证据**；钩子在 `app.run()` 之外打开菜单且不包自动释放池时足迹每次约增 2 MB，包池后不增长——这是钩子条件，不是正常事件循环下的结论。

### 预算与采样策略结论

- **40 MiB RSS 仍是已接受的暂定目标，本机未满足**（菜单未打开时已约 67 MiB）。同机空状态栏项基线与之相近只说明本机 RSS 大部分来自系统框架页，不能推出所有系统、硬件或所有 AppKit 应用都有固定 RSS 下限，也不能证明 Hostwatch 自身成本为零。
- **提议（待用户确认，未批准、未生效）**：以物理内存足迹（10^6 字节 MB）为主预算、RSS 保留为对照，足迹 ≤ 40 MB。在用户确认前，验收仍按 RSS 目标判为未通过。
- **菜单展开期间：继续采样**（定时器在 common modes）。标题、告警与历史持续更新；已展开菜单的内容是打开时的快照，重新打开才刷新；「指标限制」中向用户说明。机制由 harness 证实，真实菜单中的行为与 CPU 未实测。
- 状态栏标题仅在标题/摘要变化时更新（源码确认）。

## 弹出面板（NSPopover，2026-10-19）

`hostwatch menubar` 与 Hostwatch.app 改为原生弹出面板，取代上一节的文字菜单。证据目录：`docs/evidence/popover-2026-10-19/`。以下三类证据彼此不能替代。

### 1. 自动化（固定样本，`src/monitor.rs`、`src/panel.rs`、`src/alerts.rs`）

`fmt --check`、`clippy --all-targets -D warnings`、`test --locked`（47 单元 + 4 集成）、`build --release --locked`、`git diff --check` 全部通过。新增回归覆盖：

- 数值与图表一致：首页处理器数值 = 图表末点 = 无障碍摘要「最近」值；详情大数字同源。
- 时间戳历史：改间隔后旧样本保留各自时间戳、不算缺口；无证据的长间隔标「无数据」，记录的采集失败 / 暂停 / 睡眠（墙钟远超单调时钟）分别标注；缺失值不画、不补 0、不跨缺口连线；交换纵轴容纳 80 MiB/s。
- 缺失 ≠ 0：预热时处理器「正在建立基线」、交换图「需要至少 2 个样本」；交换源缺失显示「未知」。
- 告警：`Engine::status` 只读暴露连续计数、首个异常样本时间和最后异常证据，原有事件语义测试不变；恢复中的旧证据带触发时刻并标注「当前已回到正常范围」；内存压力 + 交换合并为一项，严重度取较高者，问题数（项）与规则数分别计数，只有组内全部规则恢复才显示已恢复；打开工具不改变恢复进度。
- 采集失败保留告警、不显示旧值；数据过时变淡并标注；部分数据缺失点名未知项。
- 暂停期间改间隔不重启采样；自定义间隔（启动参数）在设置中显示为选中；首页 / 设置 / 底栏间隔一致。
- 进程排行：未读取 → 读取中（按钮禁用、不重叠）→ 结果 / 空 / 失败；只有确实读不到的进程数（RSS 为 0）才提示受限，结果少于 5 个不推断权限问题；进程名去除控制字符。

### 2. 原生旅程（临时 harness，在正常 `NSApplication.run()` 事件循环中执行，release 代码路径）

条件：屏幕录制与辅助功能权限均未授予（`screencapture`：could not create image from display；System Events：-1719 / -25211），未更改权限，因此无法用系统级鼠标点击或截取真实屏幕。改用不进入产品代码的 harness 模块（源码 `harness-module.rs.txt`，与产品文件的差异 `harness-vs-product-menubar.diff`）：真实显示 NSPopover；每次「点击」先在弹出窗口做 `hitTest:` 确认目标按钮是最上层视图，再 `performClick:` 走 AppKit target/action；Esc 用 `NSEvent` keyDown（keyCode 53）经 `-[NSWindow sendEvent:]` 送入响应链；截图为对应视图的 `cacheDisplayInRect:` 位图（原生绘制，不是屏幕截图，不含弹出面板材质与箭头）。日志 `native-journey.log`，截图 `screens/`（进程排行结果截图与日志中的本机进程名未提交，只保留数量）。

| 旅程 | 结果 |
| --- | --- |
| 启动 | 单进程、无 Dock 图标（Accessory）；首个样本预热，结论「正在准备」 |
| 资源行 → 内存 / 处理器 / 磁盘详情 → 返回 | 三行均命中最上层按钮；Esc 与「‹ 概览」都回到首页 |
| 更多 › 进程排行 → 读取 | 读取中 → 约 3 秒内返回前 5 名（本机 1226 个进程中 397 个读不到 RSS，界面如实提示）；工作线程已 join |
| 设置：10 秒 → 暂停 → 改 5 秒 | 定时器 10 s → 无；暂停中改间隔后仍无定时器；暂停 12 秒内样本数 2 → 2 |
| 继续 | 立即取样，状态预热（基线重建）；5.6 秒后速率可用，间隔 5 s |
| 面板打开时采样 | 11 秒内样本 4 → 6 |
| 内存详情 › 打开活动监视器 | 系统接受请求，界面显示「已请求打开」及手动路径（不宣称已定位到「内存」页）；活动监视器激活后瞬态面板按系统行为关闭 |
| 面板关闭时采样 | 11 秒内样本 6 → 8 |
| 首页 Esc 关闭；重新打开 | 上一轮运行中 Esc 在首页关闭面板；重新打开回到首页并显示最新样本 |
| 更多 › 退出 | 退出码 0，无残留进程 |
| 浅色 / 深色 | `01-home-light.png`、`02-home-dark.png` 等，均为系统语义色 |

警告、严重、多项问题、问题列表、处理方法（含工具打开失败 + 手动路径 + 重试）、恢复观察、采集失败、部分缺失、数据过时的截图以 `fixture-*.png` 命名：**固定样本数据**送入真实原生渲染，不是本机实时读数。前几轮 harness 运行中出现过非脚本发起的导航和面板关闭（当时有人在使用这台 Mac，日志中当前事件为鼠标移动），已用最终一轮无干扰运行作为记录。

### 3. 资源（`scripts/proc-usage.py`，release 构建，5 秒间隔；原始数据 `perf/`）

| 场景 | RSS MiB | 足迹 MB | CPU |
| --- | --- | --- | --- |
| 面板关闭，30 分钟中的前约 24 分钟（随后该实例被外部打开过面板，见下） | 69.9–73.1 | 13.78–13.93 | 约 1.9 s / 1440 s（≈ 0.13% 单核） |
| 同一实例被打开后的剩余约 5 分钟 | 120.1 | 49.5（打开瞬间单个采样点 311.9） | — |
| 面板持续打开 60 秒（`popover-held-open.tsv`） | 102.1 | 33.3（打开瞬间单点 324.1） | 每 5 秒重绘一次，60 秒约 0.01 s |
| 200 次开合（每次 0.8 秒，含导航；`opens-200.tsv`） | 116–118.5 | 开合期间 324–377；结束后 65.0，后 165 秒持平 | 约 0.19 s / 次开合 |
| 一次按需进程扫描（`process-scan.tsv`） | 72.8 → 74.5 | +1.8 MB，随后回落 | 约 0.05 s |

`footprint -p` 显示面板打开时还有约 197 MB「可回收」的图形内存（owned physical footprint, graphics），打开瞬间与快速开合时的 `phys_footprint` 尖峰主要来自这部分；`footprint` 工具同时报告的常驻足迹约 36 MB。

结论：**RSS ≤ 40 MiB 目标未满足**（关闭时约 72 MiB）。足迹 ≤ 40 MB 仍是**待确认的提议，未批准**：按该提议，关闭（13.8 MB）和持续打开（33 MB）在线内，反复开合后（50–65 MB）超出。8 小时运行与真实睡眠唤醒**未测**。

## 未验证

- 30 分钟 / 8 小时长时间运行与实际睡眠唤醒（睡眠判定只在固定样本中验证）。
- 真实内存压力、交换风暴、CPU 饱和、磁盘将满时的告警触发（只在固定样本中验证）。
- Intel Mac、旧版 macOS（`host_statistics64` 字段数检查有降级，但未在旧系统运行）。
- 权限受限环境（沙盒、MDM）下各源失败的展示。
- 弹出面板的系统级鼠标点击与真实屏幕截图（无屏幕录制 / 辅助功能权限；harness 验证见上一节）；VoiceOver 实机朗读、全键盘访问下的 Tab 顺序、「降低透明度」、刘海屏菜单栏拥挤时的显示。
- 系统设置是否停在「储存空间」页；状态栏图标在各状态下的实际外观（SF Symbols 模板图）。
- 非 ad-hoc 签名、公证、下载后 Gatekeeper 流程的实测。
- 菜单栏受控的 30 分钟空闲（本轮 30 分钟中途有未受控 GUI 事件）/ 8 小时运行、睡眠唤醒（计时器在睡眠后的行为）、受控的多次进程排行；系统设置是否落在「储存空间」页。
- 磁盘 I/O、Windows、通知、.app 打包与签名分发均未实现。
