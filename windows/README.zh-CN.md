# Hostwatch Windows 0.4.0

双击 **Hostwatch-Windows.exe** 打开原生桌面面板。请将它与 `hostwatch.exe` 保持在同一目录。

- **资源总览**：每 5 秒刷新 CPU、物理内存、系统提交量、C 盘可用空间及可读取进程前五名。
- **C 盘分析**：按需扫描，默认 60 秒 / 30 万条目；支持指定目录、时限及迁移目标盘。可以取消，完成后可导出 JSON。
- **迁移策略**：依据已测目录区分个人文件、开发缓存、应用数据与系统组件；检查目标盘空间，提供具体处理步骤和 Robocopy `/L` 预演命令。
- **托盘**：点击“收起到托盘”继续监控；双击托盘图标恢复。关闭窗口或托盘菜单“退出”会停止采集和扫描。

本程序不会自动删除、复制或移动文件，也不会注册后台服务或开机启动。导出报告含本机路径。

统计的是文件逻辑长度，父子目录有重叠，容量不能相加，也不代表安全可释放空间。联接、云占位、权限不可读项会跳过；超时或达到条目上限时会明确标记。温度、分页读写速率与磁盘 I/O 未采集。内存提交量不等于分页文件已用量。

命令行（在程序目录打开 PowerShell）：

```powershell
.\hostwatch.exe snapshot --json --processes
.\hostwatch.exe watch --interval 5 --count 3 --json
.\hostwatch.exe analyze --path C:\ --target D:\ --seconds 60 --json
```

要求：64 位 Windows 10 / 11，系统自带的 Windows PowerShell 5.1 与 .NET Framework WPF 可用。无需管理员权限；普通权限扫描结果可能不完整。企业策略禁用 PowerShell 时可继续使用 CLI。

基于 [corn12138/hostwatch](https://github.com/corn12138/hostwatch)，保留 MIT 许可证。Windows 版本未做代码签名。
