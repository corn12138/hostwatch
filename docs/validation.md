# 本机验证记录

日期：2026-10-19。环境：macOS 27.2（26B5112a），Apple silicon（arm64），48 GiB 内存，16 KiB 内核页；Rust 1.79，`--release`（LTO、strip）。只代表这一台机器。

## 自动化检查

```sh
./scripts/cargo.sh fmt --check
./scripts/cargo.sh clippy --locked --all-targets -- -D warnings
./scripts/cargo.sh test --locked   # 21 单元 + 3 集成，全部通过
```

固定样本覆盖：首个样本 warmup、源不可用、计数器回退、睡眠（墙钟远超单调时钟）、墙钟倒退、CPU tick 32 位回绕；未知压力不算正常；warmup 时不宣称 CPU/交换健康；警告需 2 个连续样本、严重立即；未知样本不触发也不恢复；单个正常样本不恢复；冷却内抖动不重复提示、冷却后重新布防；升级立即提示；历史上限 120。

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

## 未验证

- 30 分钟 / 8 小时长时间运行与实际睡眠唤醒（睡眠判定只在固定样本中验证）。
- 真实内存压力、交换风暴、CPU 饱和、磁盘将满时的告警触发（只在固定样本中验证）。
- Intel Mac、旧版 macOS（`host_statistics64` 字段数检查有降级，但未在旧系统运行）。
- 权限受限环境（沙盒、MDM）下各源失败的展示。
- 磁盘 I/O、Windows、菜单栏、通知、签名分发均未实现。
