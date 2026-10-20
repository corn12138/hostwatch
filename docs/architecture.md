# 技术选型与架构

日期：2026-10-19。状态：初始化选用 Rust，后续资源基准与原生 UI 验证可触发复评。

## Rust 与 Go

| 维度 | Rust | Go | 本项目取舍 |
| --- | --- | --- | --- |
| 常驻内存 | 无 GC；可以精细控制分配与对象生命周期 | 有 GC 和运行时；能做低分配设计，但存在 CPU/堆内存权衡 | Rust 更符合低常驻成本优先的目标 |
| 采集效率 | 可直接调用 macOS / Windows 原生接口 | 同样可以调用原生接口；macOS 接口可能涉及 cgo | 两者均足够，采样范围和频率更关键 |
| 开发效率 | 所有权与 FFI 增加初期成本 | 编写与维护后台服务通常更直接 | 接受 Rust 的前期成本 |
| 系统 UI | 原生 UI / FFI 需要额外整合 | 原生 UI 同样需要额外整合 | 先固定核心接口，菜单栏 UI 单独验证 |
| 跨平台 | 条件编译与平台适配模块 | 构建标签与平台模块 | 两者都可支持；Windows 不影响 Mac 首版 |
| 内存上限 | 无语言级 RSS 硬上限保障，仍需约束缓冲区与采集规模 | GOMEMLIMIT 是运行时软限制，不等于总 RSS 上限 | 都必须实测，不仅看语言宣传 |

**结论：本轮选 Rust。** 这是针对本项目目标的工程判断，未做相同功能的 Rust/Go A/B 基准，不宣称 Rust 在所有场景中必然更快或占用某个固定倍数更少。

参考：

- [Rust 官方介绍](https://rust-lang.org/)：无垃圾回收器，适合对内存和运行开销敏感的代码。
- [Go 官方 GC 指南](https://go.dev/doc/gc-guide)：GC 的 CPU / 内存取舍、软内存限制，以及为什么不应用虚拟地址空间评判实际内存占用。
- [Apple 活动监视器内存说明](https://support.apple.com/guide/activity-monitor/view-memory-usage-actmntr1004/mac)：压力、缓存、压缩和交换空间应综合理解。
- [sysinfo 0.30.13](https://docs.rs/crate/sysinfo/0.30.13)：按需刷新；关闭默认多线程可避免额外资源成本。
- [XNU 内存压力实现](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_memorystatus_notify.c)：内核压力值来源，私有接口需保留降级路径。

## 实现分层

```text
CLI / 菜单栏（monitor.rs 状态 → panel.rs 页面与图表数据 → menubar.rs AppKit NSStatusItem + NSPopover）
  → Collector 平台接口
      → macOS：hw.memsize、host_statistics64（内存分项与交换/页面计数）、
               host_cpu_load_info、vm.swapusage、sysctl 压力、statfs 磁盘；
               sysinfo 仅用于可选进程排行
      → Windows：显式 unsupported 占位
  → Activity（双时钟差分；warmup / 计数器重置 / 睡眠跳变不出速率）
  → Snapshot（字节、时间、压力级别、逐源状态、采集限制）
  → Finding（每条规则：异常 / 正常 / 未知）
      → Advice（单样本建议）
      → alerts::Engine（连续样本、冷却、升级、恢复）+ 120 样本有界历史
  → 文本 / JSON 输出；菜单栏读取 Engine::active() 展示持续告警（未来通知消费 Event）
```

共享模型不泄露平台句柄。建议规则不访问文件系统、不执行操作，因此可使用固定样本回归验证。未来 Windows 必须映射自身可用指标；不能把「可用内存比例」直接伪装成 macOS 内核压力。

当前采用单线程同步采样，不引入异步运行时、数据库、网络服务或 WebView。菜单栏面板：主线程 `NSTimer`（注册在 common run-loop modes，面板或「更多」菜单打开期间也触发）驱动采样（单次采集约百微秒），状态栏图标只在结论变化时更新；面板视图只在显示时按当前状态重建（新样本、导航、命令），关闭后丢弃视图、只停绘制不停采样。页面由 `panel::page` 生成：数值、图表点、无障碍摘要来自同一份样本，图表横轴用每个样本自己的时间戳，缺口只用记录到的证据（暂停 / 采集失败 / 睡眠）标注，否则标「无数据」。恢复进度直接读取 `alerts::Engine` 的只读规则状态（`Engine::status`），按钮点击不参与。按需进程排行在一个短命工作线程中用独立的 `sysinfo::System` 扫描，主线程以 0.1 秒定时器取结果并 join，不会重叠，退出时也会等待它结束。系统工具只经 `NSWorkspace` 打开活动监视器或储存空间设置，不终止进程、不删除文件、不提权。绑定用 `objc2` 0.6 / `objc2-app-kit` 0.3（只开启所需 feature，已在 Rust 1.79 构建），不使用 Tauri / WebView。RAM / swap / CPU 走原生接口，每个采集器只持有一个 Mach host 端口；`sysinfo::System` 只在开启进程排行时刷新。`sysinfo` 固定为适配本机 Rust 1.79 的 0.30.13；后续升级需同时验证指标语义与 MSRV，不能只更新版本号。

## 性能预算（目标，不是产品保证）

| 场景 | 初始预算 | 验证方法 |
| --- | --- | --- |
| CLI 持续采样，默认 30 秒，无进程排行 | 峰值 RSS ≤ 20 MiB；平均 CPU < 单核 1% | release，至少 30 分钟，CPU 时间 / 墙钟时间 |
| 按需进程排行 | 单次 P95 ≤ 1 秒 | 多次测量，记录系统进程量与权限 |
| 菜单栏版 | 暂定 RSS ≤ 40 MiB（仍是已接受的目标）→ **本机未满足**：macOS 27.2 / arm64，5 秒间隔：面板关闭约 71–73 MiB RSS / 13.8 MB 足迹，打开约 102 MiB / 33 MB，200 次开合后约 118 MiB / 65 MB（旧文字菜单版未打开时约 67 MiB RSS），曾记录的同机空 NSStatusItem 基线 RSS 相近；这不证明其他系统、硬件或所有 AppKit 应用都有同一 RSS 下限。**提议（待用户确认，未批准）**：以物理内存足迹为主指标、RSS 保留为对照，足迹 ≤ 40 MB（10^6 字节） | 见 validation.md：条件、单位与可复跑脚本 `scripts/proc-usage.py`；包括整个产品相关进程 |
| 常驻状态保存 | 有界缓冲区 | 明确历史容量与淘汰规则，长期运行验证无增长 |

CLI 原型没有历史缓冲区，数据输出后丢弃；进程排行需要扫描进程列表，输出 Top 5 并不代表底层只采集五个进程。不要从无进程采样的数据推算完整菜单栏产品成本。

## 产品判断原则

1. 每条建议包含证据，避免「清理即可加速」的无依据保证。
2. 内存压力优先于使用率；swap 变化率、压缩页与持续时长在下一阶段补齐。
3. APFS 按容器/卷语义呈现，共享容量不得重复相加。
4. 容量监控与 I/O 繁忙分开。CPU、温度、网络或应用锁死仍可能导致卡顿。
5. 默认诊断与人工建议。未来清理必须有预览、排除规则与可恢复方案；不直接处理系统缓存、swap 或 Time Machine 快照。
6. 菜单栏优先尝试原生轻量外壳；如果评估 Tauri 等方案，必须将 WebView/辅助进程算入总预算。
