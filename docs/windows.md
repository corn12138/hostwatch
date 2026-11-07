# Windows 使用与 C 盘分析

Windows 实现使用 Rust 采集核心和原生 WPF 桌面面板，新增 **C 盘分析与迁移策略**。

## 直接使用

双击 `dist/Hostwatch-Windows-0.4.0/Hostwatch-Windows.exe`。分发包为 `dist/Hostwatch-0.4.0-windows-x64.zip`，解压后保留两个 EXE 在同一目录。

- **资源总览与托盘**：CPU、物理内存、提交量、C 盘可用空间、工作集前五名；每 5 秒刷新，普通权限运行。
- **C 盘分析**：按需扫描，支持指定目录、时间上限、条目上限；目录排行与 30 个最大文件，明确报告未完成、不可读与云占位/联接跳过数量。可取消并导出 JSON。
- **迁移策略**：按实际目录识别个人文件夹、开发缓存、应用数据、系统组件；自动选择其他固定磁盘或指定目标盘，检查容量并留出至少 10% 或 5 GiB 余量；扫描不完整时不宣称全量空间足够。
- **迁移预演**：只生成 PowerShell 格式的 Robocopy `/L` 命令，正确转义单引号，保留来源目录层级以避免同名文件夹混合。没有删除、复制、移动或创建联接的操作。
- **CLI**：保留快照、NDJSON 持续采样、持续异常告警与 120 个样本历史；新增 `analyze`。

```powershell
.\dist\Hostwatch-Windows-0.4.0\hostwatch.exe snapshot --json --processes
.\dist\Hostwatch-Windows-0.4.0\hostwatch.exe watch --interval 5 --count 3 --json
.\dist\Hostwatch-Windows-0.4.0\hostwatch.exe analyze --path C:\ --target D:\ --seconds 60 --json
.\dist\Hostwatch-Windows-0.4.0\hostwatch.exe analyze --path C:\Users\你的用户名\Downloads --max-entries 100000
```

## 构建与验证

要求 Rust 1.79+、Windows SDK / MSVC 构建工具，以及 Windows 自带的 .NET Framework 4.x / Windows PowerShell 5.1。在 Visual Studio Developer PowerShell 中：

```powershell
.\windows\build.ps1 -Check
```

脚本依次执行格式检查、Clippy、测试、release 构建、桌面宿主编译和 ZIP 打包；如果项目内 `.local/toolchain/sdk` 已备有 SDK，也可使用 Rust 自带的 LLD。CLI 使用静态 CRT，不要求额外安装 VC 运行库。桌面宿主是原生 .NET EXE，内嵌 WPF 脚本，通过系统的 PowerShell runspace 执行；没有终端窗口、浏览器或本地服务器，不修改执行策略。

Windows CI 位于 `.github/workflows/windows.yml`，输出可下载的便携 ZIP；原有 macOS CI 保留。构建产物未签名。Windows 桌面界面为中文，原有 macOS 中英文界面保留。

## 数据口径与边界

- `snapshot` 仍为 schema 2；Windows 新增可选 `snapshot.windows`，提供可用/已用物理内存、提交量/上限及系统缓存。不会用 macOS 的内存分项冒充 Windows 分项，也不把提交量当成分页文件占用。
- CPU 使用 `GetSystemTimes` 的 64 位差分；`QueryUnbiasedInterruptTime` 排除睡眠时间。首次采样、失败、计数器倒退与睡眠间隙均不输出伪造速率。超过 64 个逻辑处理器时，仅反映当前处理器组。
- 内存压力是产品启发式：可用物理内存 <10% / 5%，或提交使用率 >85% / 95% 时提醒 / 严重；并非 Windows 系统压力等级。未采集分页读写、磁盘 I/O 或温度。
- 扫描报告为 schema 1，默认 60 秒 / 30 万条目，最多允许 3600 秒 / 500 万条目。优先覆盖用户目录，再扫描系统目录；只读元数据，不读取文件内容。取消会终止当前扫描，保留上次成功报告。
- 数字是逻辑长度；父子目录重叠，不能相加。硬链接可能重复统计，压缩/稀疏文件、NTFS 元数据、卷影副本与不可读内容可能导致与磁盘属性不同。**逻辑长度不等于可回收量**。
- 跳过所有重解析点、目录联接、离线与云占位条目；不会主动下载云文件。权限不足会给出示例与计数，不把未读目录当作零占用。
- 迁移策略仅供审核；不自动清理、移动系统目录、结束进程、安装服务或添加启动项。正式迁移应使用文件夹位置页、应用自身设置或受支持的导出/导入流程，并在核验后自行处理旧副本。
- 运行时不联网；扫描结果默认只在内存中，导出报告可能含本机路径。关闭窗口会退出；“收起到托盘”会继续采样。

指标与策略参考：[Windows 内存口径](https://learn.microsoft.com/en-us/windows/win32/memory/memory-performance-information)、[CPU API](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getsystemtimes)、[Robocopy 预演参数](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/robocopy)。

实现与本机验证见 [docs/windows-validation.md](windows-validation.md)。更完整的操作说明见 [windows/README.zh-CN.md](../windows/README.zh-CN.md)。