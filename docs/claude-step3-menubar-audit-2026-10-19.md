# Claude 第 3 步菜单栏交付复核

审计日期：2026-10-19（America/New_York）。审计对象：`main`，HEAD `81d8fa6` 之上的现有未提交改动，包含未跟踪的 `src/menu.rs`、`src/menubar.rs`。本次只增加本报告，没有修改实现、提交代码或触碰 `docs/juejin/`。

## 结论

**部分通过：原生菜单栏 MVP 已实现，现有工程检查全部通过；第 3 步不能按完整验收通过处理。**

有一个独立复现的状态显示缺陷，以及原需求中的操作入口缺失。资源短测支持“当前版本开销较低、该短测窗口未持续增长”，不支持“所有 AppKit 应用都无法满足 RSS 门槛”或“长期无泄漏”的普遍结论。原 RSS 预算仍未满足；改成 footprint 的建议尚未获用户确认。

## Findings

### P2：CPU 不可用时状态栏可能显示正常

- 证据：`src/menu.rs:179–187` 仅检查源错误、未知内存压力和整体 `RateStatus`，随后返回 `Health::Normal`，未检查 CPU 利用率是否存在。
- 实际可达路径：`src/activity.rs:65–74` 在 VM 计数正常、上一采样的 CPU ticks 缺失、本次 CPU ticks 恢复时，产生 `Ready` 加 `cpu_busy_percent = None`。本次数据源均成功，因而不触发菜单的 Partial 分支。CPU ticks 差分为零也可能产生此组合。
- 独立复现：使用实际 `activity::derive` 构建上述样本；菜单详情是 `CPU：不可用`，标题却是 `HW ✓`，摘要为“未见持续异常”。隔离副本中的回归断言失败，原工作区没有加入测试或修复。
- 影响：恢复边界的一个采样周期内，整体状态掩盖仍不可判断的 CPU 指标；默认间隔为 30 秒，自定义最大 3600 秒。与“部分数据不可用而非正常”的交付承诺不符。
- 修复要求：菜单状态应考虑必需指标的实际可用性；无 CPU 差分时显示恰当的预热或部分不可用状态，摘要也应指出 CPU，不能误写为内核内存压力未知。保留已有持续告警及恢复策略。

### P2：建议操作入口仍缺失，第 3 步却标为完成

- 原需求证据：`git show HEAD:docs/roadmap.md` 的第 3 步明确包含“建议（证据、操作入口）”。
- 实现证据：`src/menu.rs:19–24` 只有间隔、暂停、进程刷新和退出四类 Action；`src/menu.rs:328–387` 只生成建议文字。`src/menubar.rs:151–191` 没有打开活动监视器等操作。
- 文档证据：当前 `docs/roadmap.md:25–36` 同时声称第 3 步完成、又列出操作入口未完成。
- 影响：用户看到建议后还需自行寻找系统工具，原定交互没有闭环。Claude 已披露缺口，这属于未完成需求，不属于隐瞒实现情况。
- 修复要求：补齐适用建议的只读系统工具入口并处理启动失败；或由用户明确缩减本阶段范围。在此之前，第 3 步应为部分完成。通知、签名、登录项等不据此扩入本轮修复。

### P2：内存预算结论超出了证据范围

- 证据：`docs/validation.md:60` 和 `docs/architecture.md` 性能预算行，把一台机器上的空状态栏基线推广为“任何 AppKit 菜单栏进程都不可达”。
- 空基线与 Hostwatch 的 RSS 接近，不足以证明 67 MiB 全是共享页、Hostwatch 自身完全没有内存成本，也不能代表所有系统版本、硬件和实现。
- 本轮原版进程起步读数约 67.1 MiB RSS、`vmmap` footprint 11.8M；后续约 99 MiB RSS、26 MB footprint。足以确认该版本本机超过原 40 MiB RSS 目标。期间 GUI 交互没有受控，不能把变化唯一归因于菜单开启。
- [Apple WWDC22 的内存说明](https://developer.apple.com/videos/play/wwdc2022/10106/?time=427) 支持使用 footprint 评估应用成本，并说明 resident memory 包含可回收的 clean pages，footprint 可通过 `proc_pid_rusage` 读取。但该说明没有给出 AppKit 的固定 RSS 下限。
- 修复要求：收窄为本机、本版本、本次场景结论；footprint 可作为主指标的建议，仍保留 RSS 对照，明确 MB/MiB 单位。用户确认前不得将新预算写成已批准或已验收。

## 实现与验收矩阵

| 项目 | 判定 | 本轮证据与边界 |
| --- | --- | --- |
| 原生 AppKit、无 WebView | Pass（源码及进程） | objc2 依赖、NSStatusItem、Accessory 策略；启动期间只有一个 hostwatch 进程 |
| 核心复用 | Pass（源码） | collector/advice/history 本次未改；Engine 只新增只读 active() |
| 状态标题避免重复设置 | Pass（源码） | `src/menubar.rs:106–124` 比较完整 headline，相同则返回；短测 CPU 很低，未重做旧版 A/B |
| 总览、告警证据建议、诊断、设置 | Partial | 视图模型和 AppKit 映射存在；自动化覆盖主要为视图模型；操作入口缺失 |
| 整体采集失败不显示旧总览、不清告警 | Pass（固定样本） | 现有菜单测试通过；缓存排行明确标有历史时间，不冒充当前数据 |
| 数据源降级 | Partial | 普通失败测试通过；CPU 恢复边界复现错误正常状态 |
| 暂停、恢复新基线、退出 | Partial | 源码具备 timer invalidate、重建 collector、terminate；本轮未实际点击验收 |
| CLI 兼容 | Pass（已测范围） | 27 单元及 4 集成通过；另外跑 release watch 两个样本；CLI-only 参数被拒绝 |
| Rust 1.79 | Pass | 当前 rustc 1.79.0，release 构建成功 |
| 40 MiB RSS 预算 | Fail（本机读数） | 启动阶段已超过；改 footprint 仍待决定 |
| 200 次菜单开合、12,000 次重建 | 未独立复验 | 本仓库没有所述临时测试钩子及可重跑脚本；已有文档保留为 Claude 自述证据 |
| UI 截图、真实点击 | Blocked | 本轮 CUA 按二进制绝对路径和 hostwatch 名称均返回 Invalid app；列表无法定位裸程序。不能据此推断是权限不足 |
| 30 分钟、8 小时、睡眠唤醒 | 未验证 | 本轮只做短测；不将固定样本睡眠测试替代真机睡眠 |
| 通知、打包签名、Windows | 后续范围 | 本次不把未实现的后续里程碑计作菜单实现回归 |

另有已披露的产品限制：`src/menubar.rs:138–145` 使用默认 run-loop 模式的 scheduled timer。菜单展开跟踪期间不会采样，且菜单只在打开时重建，长时间阅读菜单会造成监测空档。`docs/validation.md:62` 已说明这一点；它应在下一轮明确作为可接受的限制或修复任务，不能宣传为菜单展开时仍实时监控。[Apple Timer 文档](https://developer.apple.com/documentation/foundation/timer) 说明了 scheduled timer 的默认模式。

## 独立执行记录

环境：macOS 27.2 / Darwin 27.2.0，arm64；rustc 1.79.0。使用仓库脚本避开路径冒号导致的动态库搜索路径问题。

```sh
./scripts/cargo.sh fmt --check
./scripts/cargo.sh clippy --locked --all-targets -- -D warnings
./scripts/cargo.sh test --locked
./scripts/cargo.sh build --locked --release
git diff --check
```

以上全部通过：27 个单元测试、4 个集成测试。新增审计回归在隔离副本中单独执行，结果为 **1 failed**，不是原有测试失败。

### CPU 恢复缺陷复现

以下代码加入 `src/menu.rs` 已有 `tests` 模块即可复现（本轮只加入临时副本）。测试调用生产中的差分逻辑，而非直接硬编码一个不一致状态。

```rust
#[test]
fn audit_cpu_recovery_must_not_report_normal_without_a_rate() {
    use crate::activity::{self, Counters, Stamp};
    use std::time::Duration;
    let old = Counters {
        page_size: 16384, swap_ins: 0, swap_outs: 0,
        page_ins: 0, cpu_ticks: None,
    };
    let new = Counters { cpu_ticks: Some([10, 10, 80, 0]), ..old };
    let stamp = |n| Stamp {
        monotonic: Duration::from_secs(n), wall: Duration::from_secs(n),
    };
    let mut s = snapshot(Pressure::Normal, 300 * GIB);
    s.activity = activity::derive(Some((stamp(0), old)), Some((stamp(30), new)));
    assert_eq!(s.activity.status, RateStatus::Ready);
    assert_eq!(s.activity.cpu_busy_percent, None);
    s.sources.push(SourceStatus {
        source: "host_cpu_load_info", ok: true, elapsed_micros: 1, error: None,
    });
    let mut monitor = Monitor::new(30);
    monitor.record(Ok(s), 30);
    assert!(texts(&monitor.view(30)).contains(&"CPU：不可用".to_string()));
    assert_ne!(monitor.health(), Health::Normal,
        "CPU rate is unavailable after source recovery; headline={:?}",
        monitor.headline());
}
```

执行 `cargo test --locked audit_cpu_recovery_must_not_report_normal_without_a_rate -- --nocapture`。本轮结果：

```text
assertion `left != right` failed:
CPU rate is unavailable after source recovery;
headline=("HW ✓", "未见持续异常")
left: Normal
right: Normal
test result: FAILED. 0 passed; 1 failed; 27 filtered out
```

### 资源短测与退出边界

运行原版 release 二进制 `hostwatch menubar --interval 5`，PID 83269，约 3 分 46 秒后结束；无新增钩子。该实例已用 SIGTERM 清理，退出状态 143，随后 `pgrep -fl hostwatch` 无结果。**这只证明审计清理无残留，不是菜单 Quit 的退出码 0 验证。**

从运行中的进程取 `proc_pid_rusage` v0，连续 120.042 秒、每 10 秒取一次，共 13 次：RSS 98.922–99.047 MiB，footprint 25.986–26.494 MB，窗口内未持续增长。用户/系统 CPU 时间原值按 Mach timebase 125/3 换算，增量约 0.0818 秒，平均约单核 0.068%。GUI 交互未受控，期间还做过 leaks 诊断，因此不将这组数字标为“菜单从未打开的纯空闲基准”。

| 相对秒数 | RSS MiB | Footprint MB |
| --- | --- | --- |
| 0.001 | 99.047 | 26.494 |
| 10.004 | 98.984 | 26.396 |
| 20.006 | 98.938 | 26.379 |
| 30.011 | 98.969 | 26.412 |
| 40.016 | 98.922 | 26.347 |
| 50.018 | 98.922 | 26.347 |
| 60.024 | 98.922 | 26.347 |
| 70.028 | 98.922 | 26.347 |
| 80.032 | 98.922 | 26.347 |
| 90.034 | 98.922 | 26.347 |
| 100.035 | 98.922 | 26.347 |
| 110.040 | 98.969 | 26.396 |
| 120.043 | 98.922 | 25.986 |

`leaks 83269` 输出 `0 leaks for 0 total leaked bytes`，同时提示进程不可调试、受安全限制只能展示/保存只读内存内容。本轮保留这个限制，不将一次 leaks 结果等同于完整的长期泄漏证明。

另跑 `/usr/bin/time -l <release-binary> watch --interval 5 --count 2 --json`：5.02 秒，最大 RSS 7,192,576 bytes（约 6.86 MiB），peak footprint 2,032,048 bytes，user 0.00 秒、sys 0.01 秒。仅证明该短测 CLI 路径仍低开销。

## 下一轮范围与验收

1. 优先修复 CPU 恢复和零差分时的错误正常状态，新增有实际失败证据的回归；保留持续告警、冷却与已有 CLI 语义。
2. 补齐原第 3 步的适用建议操作入口，失败要可见；不得终止进程、清理文件或执行提权操作。未补齐前纠正阶段完成标记。
3. 收窄性能结论；保留空基线及开合测试的可复跑方式、原始日志、准确单位、构建版本与 event-loop/autorelease-pool 条件。不得用测试钩子的成功替代真实 UI 验收。
4. 真实 UI 验证覆盖总览、建议、诊断、排行、间隔勾选、暂停期间无采样、继续首个样本预热、菜单退出码 0 及无残留；无法执行时给出具体工具或权限阻塞。
5. 明确菜单展开期间的采样策略。若继续停止采样，文档和用户可见提示都需准确；若改为持续采样，验证事件循环、打开中菜单的稳定性和 CPU。
6. 第 5 步的长测继续单独跟踪：至少 30 分钟，以及计划中的 8 小时、睡眠唤醒、重复排行、Intel；本轮不要求为了补第 3 步同时实现通知、Windows 或发行。

最终复核门槛：原有全部检查通过、新增回归由失败变通过、必要 UI 验收有证据、原需求缺口关闭、性能预算仍准确标注决策状态。没有用户确认不得擅自把 footprint 建议替换成已批准预算。

## English Claude prompt

```text
Read AGENTS.md if present and the user-provided repository instructions first.
Then read docs/claude-step3-menubar-audit-2026-10-19.md in full, followed by
docs/roadmap.md, docs/architecture.md and docs/validation.md.

Close the Step 3 gaps described in this audit. First reproduce and fix the
CPU-unavailable/normal-headline bug with a meaningful regression test. Complete
the originally required read-only advice action entry points, preserving the
collector, alert/cooldown semantics and existing CLI behavior. Resolve the
documented menu-tracking sampling policy and report its actual behavior.

Preserve all inherited changes and docs/juejin/. Do not commit, reset, stash,
push, or expand this work into notifications, Windows, signing or distribution.
Retain reproducible performance evidence with correct units and explicit
event-loop/autorelease-pool conditions. Restrict memory conclusions to measured
platforms and scenarios. A footprint budget is a proposal awaiting user approval;
do not silently change the accepted RSS target or claim that all AppKit apps
share a fixed RSS floor.

Use scripts/cargo.sh for fmt, clippy, tests and release build. Verify real menu
actions and clean exit where the environment permits; otherwise identify the
exact blocker and leave those checks unpassed. Distinguish source inspection,
unit tests, temporary harness results, real UI checks and long-duration evidence.
Report final changes, regression results, remaining gaps and whether Step 3 is
actually ready for acceptance.
```
