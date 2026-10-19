# 初始化状态与下一阶段任务

日期：2026-10-19。源需求：低开销监测 Mac / Windows 的内存和磁盘，及时帮助定位卡顿并给出优化建议；Mac 优先，Windows 预留。

## 阶段判定

本轮交付是 macOS CLI 原型与工程初始化，不是完成的桌面产品。当前没有需要审计的网页、菜单栏窗口或历史页面。

| 能力 | 本轮状态 | 后续缺口 |
| --- | --- | --- |
| 语言选型 | Rust；原因见 architecture.md | 相同负载 Rust / Go A/B 基准尚未实施 |
| 内存/交换/压力 | 原生分项、压缩、交换速率、逐源状态；本机验证见 validation.md | 活动监视器逐项对照、Intel 验证 |
| 磁盘空间 | 启动 Data 卷 | 外置盘、APFS 容器展示、目录归因 |
| CPU | 总体占用（tick 差分） | 按进程 CPU、温度/降频 |
| 磁盘 I/O | 未实现 | IOKit 块设备统计、延迟 |
| 进程定位 | 可选 RSS Top 5 | 应用级聚合、CPU/磁盘 I/O、权限覆盖 |
| 优化建议 | 内存压力、交换读写、CPU 饱和、磁盘空间四条规则 | 处理后复查、按应用的建议 |
| 及时提醒 | watch 内持续判定、冷却、升级与恢复事件；120 样本趋势 | 原生通知 |
| 常驻菜单栏 | 未实现 | UI、退出/暂停、后台生命周期 |
| Windows | 代码接口预留，运行时明确未支持 | 采集、托盘、平台建议与真机验证 |
| 打包发行 | 未实施 | 签名、公证、安装、升级、卸载 |

## 下一阶段：Mac 菜单栏 MVP

按以下依赖顺序推进，先补诊断可信度，再接 UI 和通知。第 1、2 步已完成；磁盘 I/O 采集留作第 1 步的补充项。

1. ✅ **采集可信度**（本轮完成，磁盘 I/O 除外）：RAM/swap 原生采集逐项返回成功/失败/时间；增加压缩页、swap-in/out 差分、CPU 利用率与可获得的磁盘 I/O 指标。对计数器重置、睡眠唤醒、权限缺失和未知压力分别测试。
2. ✅ **持续异常与证据**（本轮完成）：至少连续两个有效样本才发警报（严重事件可定义单独策略）；同类事件 10 分钟冷却、恢复后重新布防。历史环形缓冲区上限 120 样本；采集失败不能当作恢复。
3. **原生菜单栏**（下一步）：总览（压力、容量、CPU）；诊断详情（趋势、应用排行、指标限制）；建议（证据、操作入口）；设置（频率、通知、暂停、退出）。必须覆盖 loading / 正常 / 异常 / 采集失败 / 权限受限状态。
4. **系统通知**：异常等级、触发证据、打开详情；申请通知权限，拒绝权限后菜单栏仍可用。冷却期间不重复打扰。
5. **性能验收**：30 分钟正常负载，额外测试 8 小时与睡眠唤醒；记录 RSS、物理内存足迹、CPU、采集耗时和系统环境。平台和 UI 辅助进程一并统计。

验收门槛：

- 正常缓存与历史 swap 不触发「必须清理」提醒。
- 同一异常不持续刷通知；未知样本不会清除异常状态。
- 菜单栏退出后无遗留采集进程；暂停确实停止采样。
- 不需要管理员权限就能完成基本诊断；受限项明确说明。
- 活动监视器对照记录解释 RAM / RSS / footprint 的差异，不能只比数字接近。
- 发布前至少覆盖 Apple Silicon 和 Intel Mac；本次仅能证明当前机器。

## 后续阶段

- **按需空间分析**：用户选定目录，限制扫描深度/预算、可取消、默认不跟随符号链接；区分表观大小和分配大小，处理 APFS 克隆/权限失败。明确列出大文件与可人工审核的清理候选。
- **Windows**：实现 Collector；优先评估 GlobalMemoryStatusEx / GetPerformanceInfo、GetDiskFreeSpaceExW 等原生接口；CPU/I/O 与进程权限独立验收。托盘/通知在 Windows 真机完成，不用 macOS 编译成功替代。
- **发行**：确定产品名与图标，签名公证、更新和卸载。自动清理不属于当前 MVP 承诺。

## English implementation handoff

> Read AGENTS.md if present, then README.md, docs/architecture.md, docs/validation.md and this task document. Preserve inherited work and the read-only behavior. Implement the next macOS milestone in dependency order: reliable per-metric collection and freshness, bounded diagnostic history, sustained-event rules, native menu-bar UI, then notifications. Do not represent unavailable metrics as healthy. Preserve the Windows adapter boundary without fabricated values. Validate cache/swap semantics, APFS capacity accounting, sleep/wake, permission failures, cooldown/recovery behavior, shutdown, and whole-product resource use. Use scripts/cargo.sh in colon-containing checkout paths. Distinguish local test evidence from Intel/Windows, notification permission, signed distribution and long-duration acceptance. Report remaining gaps with reproducible evidence.
