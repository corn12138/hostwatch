# 竞品对照（2026-11-03）

目的：弄清同类工具各自擅长什么，Hostwatch 在哪里能赢、哪里不必跟。数据来自各自官网 / 介绍文章与本机实测；第三方评测站点质量参差，价格以官网为准。

## 一览

| 工具 | 定位 | 价格 / 许可 | 取数方式 | 自身开销 |
| --- | --- | --- | --- | --- |
| Mole（tw93） | 清理 + 软件管理 + 状态面板 | Mac 版 $19 买断，CLI 开源（GPL-3.0） | 原生 SwiftUI + IOKit | 本机实测：首次打开窗口 3 分钟平均 32% 单核、足迹 183–478 MB；隐藏 5 分钟平均 13% 单核、足迹最高 634 MB（含首次扫描） |
| System Sentinel | 菜单栏「医生」：白话说明 + 指出进程 + 一键处理（可撤销） | 免费，MIT | Swift，调用 `ps`、`netstat`、`pmset` 等命令 | 作者在 M4 Air 实测约 3% 单核 |
| iStat Menus 7 | 可深度定制的菜单栏读数 | App Store $11.99（另有天气订阅），Setapp 可用 | 原生 | 未测 |
| Stats（exelban） | 免费开源的菜单栏读数 | 免费，MIT | 原生 | 未测 |
| 活动监视器 | 系统自带，按需排查 | 免费 | 原生 | — |
| **Hostwatch** | 只读诊断：一句结论 + 证据 + 谁造成的 | 免费，MIT | Rust + 原生 libproc / host_statistics | 本机实测：足迹约 15 MB、约 0.07% 单核（5 秒间隔） |

## 各家要点

- **Mole**：功能最全（清理、卸载、更新、优化、磁盘树图、状态）。状态页有健康分（内存压力、处理器、磁盘、磁盘 I/O、温度、电池扣分）与「为什么在运行」（进程由谁启动、是否阻止睡眠、读写量）。通知只覆盖磁盘空间、摄像头 / 麦克风与蓝牙电量，不对内存、处理器或过热发通知。
- **System Sentinel**（2026-10-19 发布）：与 Hostwatch 理念最接近。每 5 秒采样，白话说明（如「WindowServer 是平时的 10.9 倍」「有 7 个东西在阻止睡眠」），进程页有 15 分钟 / 1 小时 / 6 小时平均；提供结束进程、降低优先级、自动结束规则、WindowServer 相关系统设置修改等操作，均有确认与撤销；有天 / 周趋势与事件日志导出。仅 Apple 芯片发行版、未公证。
- **iStat Menus**：规则（Rules）可配置通知，如「CPU 超过 60% 持续 10 秒」「公网 IP 变化」；强在菜单栏读数的定制，不解释原因。
- **Stats**：模块齐全（CPU、GPU、内存、磁盘含读写、网络、电池、传感器、蓝牙），只显示读数。

## Hostwatch 的优势（继续加强）

1. **开销小一个数量级**：原生内核计数，不调用外部命令；足迹约 15 MB。值得持续用 `scripts/proc-usage.py` 复测并写进 README。
2. **判断可信**：连续样本规则、冷却、未知不当作正常、恢复由样本判定；通知只在引擎新发出或升级时发。
3. **说清楚是谁**：通知与处理方法页写出最重的应用、问题开始前上升最多的应用或后台进程，以及持续占满一个核心的单个进程（这次新增，整机未满时也能发现）。
4. **安全**：不强制结束、不删除文件、不改系统设置；文件夹统计只读。System Sentinel 与 Mole 都会结束进程或删除文件，Hostwatch 刻意不做。

## 差距（按价值排序）

1. ~~阻止睡眠的进程~~（2026-11-03 已做：「更多 › 什么在阻止睡眠」，只读）。
2. **磁盘 I/O**（Mole 健康分、Stats、Mole 文章列为常见原因）：`proc_pid_rusage` 已含每进程读写字节，可进记录，回答「谁在狂写磁盘」。
3. **更长的历史与趋势**（System Sentinel 天 / 周）：与「不持久保存」的承诺冲突，需先定是否可选开启。
4. **菜单栏数字读数**（iStat Menus、Stats、Mole）：可选的紧凑读数，非诊断核心。

不跟的：清理 / 卸载 / 应用更新（Mole 已做好，且违背只读承诺）、自动结束进程（与安全定位冲突）、风扇控制、天气。

## 来源

- Mole：<https://mole.fit/releases>、<https://mole.tw93.fun/blog/why-is-my-mac-so-slow>；本机实测见本次会话（2026-11-02，1.18.0，公证的 Developer ID 版本）。
- System Sentinel：<https://andrewbaker.ninja/2026/10/19/system-sentinel-a-menu-bar-doctor-for-your-mac/>
- iStat Menus：<https://bjango.com/help/istatmenus7/rules/>、<https://apps.apple.com/app/id6499559693>
- Stats：<https://project-awesome.org/r/exelban-stats>
- 综述：<https://cindori.com/reviews/best-istat-menus-alternatives-mac>
