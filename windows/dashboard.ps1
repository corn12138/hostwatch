param([Parameter(Mandatory=$true)][string]$Backend)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework,PresentationCore,WindowsBase,System.Windows.Forms,System.Drawing
$script:Jobs = @{}
$script:ScanReport = $null
$script:History = [System.Collections.Generic.List[double]]::new()
$script:Closing = $false
function Quote-Argument([string]$Value) { '"' + ($Value -replace '(\\*)"', '$1$1\"' -replace '(\\+)$', '$1$1') + '"' }
function Start-Backend([string]$Kind,[string[]]$Arguments) {
    if ($script:Jobs.ContainsKey($Kind)) { return }
    $info = [System.Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Backend
    $info.Arguments = ($Arguments | ForEach-Object { Quote-Argument $_ }) -join ' '
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [System.Text.UTF8Encoding]::new($false)
    $info.StandardErrorEncoding = [System.Text.UTF8Encoding]::new($false)
    $process = [System.Diagnostics.Process]::new(); $process.StartInfo = $info
    [void]$process.Start()
    $script:Jobs[$Kind] = @{ Process=$process; Out=$process.StandardOutput.ReadToEndAsync(); Err=$process.StandardError.ReadToEndAsync(); Started=[DateTime]::Now }
}
function Size([double]$Bytes) {
    if ($Bytes -ge 1GB) { return ('{0:N2} GiB' -f ($Bytes / 1GB)) }
    if ($Bytes -ge 1MB) { return ('{0:N1} MiB' -f ($Bytes / 1MB)) }
    return ('{0:N0} KiB' -f ($Bytes / 1KB))
}
[xml]$xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Title="Hostwatch · Windows" Width="1160" Height="810" MinWidth="960" MinHeight="670" WindowStartupLocation="CenterScreen" Background="#0C111B" Foreground="#E7EDF6" FontFamily="Segoe UI, Microsoft YaHei UI" FontSize="13">
<Window.Resources>
 <SolidColorBrush x:Key="Panel" Color="#141D2B"/>
 <Style TargetType="Button"><Setter Property="Background" Value="#223149"/><Setter Property="Foreground" Value="#E7EDF6"/><Setter Property="BorderThickness" Value="0"/><Setter Property="Padding" Value="16,10"/><Setter Property="Margin" Value="0,0,8,0"/><Setter Property="Cursor" Value="Hand"/><Setter Property="Template"><Setter.Value><ControlTemplate TargetType="Button"><Border Background="{TemplateBinding Background}" CornerRadius="6" Padding="{TemplateBinding Padding}"><ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/></Border><ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter Property="Opacity" Value="0.8"/></Trigger><Trigger Property="IsEnabled" Value="False"><Setter Property="Opacity" Value="0.4"/></Trigger></ControlTemplate.Triggers></ControlTemplate></Setter.Value></Setter></Style>
 <Style TargetType="TextBox"><Setter Property="Background" Value="#0C1420"/><Setter Property="Foreground" Value="#E7EDF6"/><Setter Property="BorderBrush" Value="#304059"/><Setter Property="Padding" Value="10,8"/><Setter Property="CaretBrush" Value="White"/></Style>
 <Style TargetType="DataGrid"><Setter Property="Background" Value="#141D2B"/><Setter Property="Foreground" Value="#D6E1EF"/><Setter Property="BorderThickness" Value="0"/><Setter Property="RowBackground" Value="#141D2B"/><Setter Property="AlternatingRowBackground" Value="#182335"/><Setter Property="GridLinesVisibility" Value="None"/><Setter Property="HeadersVisibility" Value="Column"/><Setter Property="RowHeight" Value="34"/><Setter Property="AutoGenerateColumns" Value="False"/><Setter Property="IsReadOnly" Value="True"/><Setter Property="CanUserAddRows" Value="False"/><Setter Property="SelectionMode" Value="Single"/><Setter Property="EnableRowVirtualization" Value="True"/></Style>
 <Style TargetType="DataGridColumnHeader"><Setter Property="Background" Value="#202D41"/><Setter Property="Foreground" Value="#93A6BF"/><Setter Property="Padding" Value="10"/><Setter Property="BorderThickness" Value="0"/></Style>
 <Style TargetType="DataGridCell"><Setter Property="BorderThickness" Value="0"/><Setter Property="Padding" Value="8,0"/><Setter Property="VerticalContentAlignment" Value="Center"/></Style>
 <Style TargetType="TabItem"><Setter Property="Foreground" Value="#182335"/><Setter Property="Padding" Value="16,8"/></Style>
</Window.Resources>
<Grid><Grid.ColumnDefinitions><ColumnDefinition Width="190"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions>
 <Border Background="#101824" BorderBrush="#233047" BorderThickness="0,0,1,0"><DockPanel Margin="18,28,10,24">
  <StackPanel DockPanel.Dock="Top"><TextBlock Text="◉ HOSTWATCH" FontWeight="Bold" FontSize="19" Foreground="#79DAC4"/><TextBlock Text="WINDOWS EDITION" FontSize="10" Foreground="#667F9D" Margin="1,7,0,38"/>
   <Button x:Name="NavHome" Content="◫   资源总览" HorizontalContentAlignment="Left" Margin="0,0,0,10" Background="#25473F"/>
   <Button x:Name="NavScan" Content="▤   C 盘分析" Margin="0,0,0,10"/>
   <Button x:Name="NavPlan" Content="⇄   迁移策略" Margin="0,0,0,10"/>
   <Button x:Name="NavInfo" Content="ⓘ   数据说明" Margin="0,0,0,10"/>
  </StackPanel>
  <StackPanel DockPanel.Dock="Bottom" VerticalAlignment="Bottom"><TextBlock Text="本地采集 · 按需扫描" Foreground="#8CA0BA" Margin="0,0,0,8"/><TextBlock Text="不上传文件与诊断数据" Foreground="#60738F" FontSize="11"/><Button x:Name="ToTray" Content="收起到托盘" Margin="0,18,0,0"/><TextBlock Text="关闭窗口将退出程序" Foreground="#60738F" FontSize="10" Margin="0,10,0,0"/></StackPanel>
 </DockPanel></Border>
 <Grid Grid.Column="1" Margin="30,25,30,22"><Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="*"/><RowDefinition Height="Auto"/></Grid.RowDefinitions>
  <Grid Margin="0,0,0,22"><StackPanel><TextBlock x:Name="PageTitle" Text="资源总览" FontSize="27" FontWeight="SemiBold"/><TextBlock x:Name="PageSubtitle" Text="看清当前负载，再决定是否需要处理。" Foreground="#8FA3BE" Margin="0,8,0,0"/></StackPanel><TextBlock x:Name="LiveTime" Text="正在连接本机…" HorizontalAlignment="Right" VerticalAlignment="Center" Foreground="#79DAC4"/></Grid>
  <Grid x:Name="HomePage" Grid.Row="1"><Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="*"/></Grid.RowDefinitions>
   <UniformGrid Columns="4" Margin="0,0,-12,20">
    <Border Background="{StaticResource Panel}" CornerRadius="10" Padding="18" Margin="0,0,12,0"><StackPanel><TextBlock Text="处理器" Foreground="#8FA3BE"/><TextBlock x:Name="CpuValue" Text="—" FontSize="30" Margin="0,14,0,4"/><TextBlock Text="两次采样间的平均负载" FontSize="10" Foreground="#6D839F"/><ProgressBar x:Name="CpuBar" Height="4" Margin="0,18,0,0" Background="#26344B" Foreground="#79DAC4" BorderThickness="0"/></StackPanel></Border>
    <Border Background="{StaticResource Panel}" CornerRadius="10" Padding="18" Margin="0,0,12,0"><StackPanel><TextBlock Text="物理内存" Foreground="#8FA3BE"/><TextBlock x:Name="MemValue" Text="—" FontSize="30" Margin="0,14,0,4"/><TextBlock x:Name="MemDetail" Text="正在读取" FontSize="10" Foreground="#6D839F"/><ProgressBar x:Name="MemBar" Height="4" Margin="0,18,0,0" Background="#26344B" Foreground="#9EABFF" BorderThickness="0"/></StackPanel></Border>
    <Border Background="{StaticResource Panel}" CornerRadius="10" Padding="18" Margin="0,0,12,0"><StackPanel><TextBlock Text="C 盘可用" Foreground="#8FA3BE"/><TextBlock x:Name="DiskValue" Text="—" FontSize="25" Margin="0,18,0,7" Foreground="#79DAC4"/><TextBlock x:Name="DiskDetail" Text="正在读取" FontSize="10" Foreground="#6D839F"/><ProgressBar x:Name="DiskBar" Height="4" Margin="0,18,0,0" Background="#26344B" Foreground="#79DAC4" BorderThickness="0"/></StackPanel></Border>
    <Border Background="{StaticResource Panel}" CornerRadius="10" Padding="18" Margin="0,0,12,0"><StackPanel><TextBlock Text="系统提交量" Foreground="#8FA3BE"/><TextBlock x:Name="CommitValue" Text="—" FontSize="25" Margin="0,18,0,7"/><TextBlock x:Name="CommitDetail" Text="包含内存承诺，非分页文件大小" FontSize="10" Foreground="#6D839F"/><ProgressBar x:Name="CommitBar" Height="4" Margin="0,18,0,0" Background="#26344B" Foreground="#E2B876" BorderThickness="0"/></StackPanel></Border>
   </UniformGrid>
   <Border Grid.Row="1" Background="#172D2B" CornerRadius="9" Padding="18" Margin="0,0,0,22"><StackPanel><TextBlock x:Name="HealthTitle" Text="正在采集当前状态" FontSize="16" Foreground="#91E1CB"/><TextBlock x:Name="HealthDetail" Text="首次采样约需 1 秒。" TextWrapping="Wrap" Foreground="#A7BDB8" Margin="0,8,0,0"/></StackPanel></Border>
   <Grid Grid.Row="2"><Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="240"/></Grid.ColumnDefinitions>
    <Border Background="{StaticResource Panel}" CornerRadius="10" Padding="18" Margin="0,0,16,0"><DockPanel><TextBlock DockPanel.Dock="Top" Text="内存占用前五名" FontSize="16" Margin="0,0,0,7"/><TextBlock DockPanel.Dock="Top" Text="可读取进程的工作集 · 共享内存可能重复" Foreground="#7188A6" FontSize="11" Margin="0,0,0,15"/><DataGrid x:Name="ProcessGrid"><DataGrid.Columns><DataGridTextColumn Header="进程" Binding="{Binding Name}" Width="*"/><DataGridTextColumn Header="PID" Binding="{Binding Pid}" Width="70"/><DataGridTextColumn Header="工作集" Binding="{Binding Size}" Width="110"/></DataGrid.Columns></DataGrid></DockPanel></Border>
    <Border Grid.Column="1" Background="{StaticResource Panel}" CornerRadius="10" Padding="18"><StackPanel><TextBlock Text="给 C 盘留点空间" FontSize="16"/><TextBlock Text="找出大目录，区分个人文件、缓存与系统组件，生成适合当前磁盘的迁移方案。" TextWrapping="Wrap" Foreground="#8FA3BE" Margin="0,14,0,20" LineHeight="23"/><Button x:Name="QuickScan" Content="分析 C 盘 →" Background="#347965"/><Button x:Name="StorageSettings" Content="打开 Windows 存储" Margin="0,12,8,0"/><Button x:Name="TaskManager" Content="打开任务管理器" Margin="0,12,8,0"/><TextBlock Text="扫描只在点击后运行。&#10;不会自动清理或搬移文件。" TextWrapping="Wrap" Foreground="#6D839F" FontSize="11" Margin="0,20,0,0"/></StackPanel></Border>
   </Grid>
  </Grid>
  <Grid x:Name="ScanPage" Grid.Row="1" Visibility="Collapsed"><Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="Auto"/><RowDefinition Height="*"/></Grid.RowDefinitions>
   <Border Background="{StaticResource Panel}" Padding="18" CornerRadius="9"><StackPanel><Grid><Grid.ColumnDefinitions><ColumnDefinition Width="*"/><ColumnDefinition Width="130"/><ColumnDefinition Width="140"/></Grid.ColumnDefinitions><StackPanel Margin="0,0,14,0"><TextBlock Text="扫描目录" Foreground="#8FA3BE" Margin="0,0,0,7"/><TextBox x:Name="ScanRoot" Text="C:\"/></StackPanel><StackPanel Grid.Column="1" Margin="0,0,14,0"><TextBlock Text="时限（秒）" Foreground="#8FA3BE" Margin="0,0,0,7"/><TextBox x:Name="ScanSeconds" Text="60"/></StackPanel><StackPanel Grid.Column="2"><TextBlock Text="迁移目标盘" Foreground="#8FA3BE" Margin="0,0,0,7"/><ComboBox x:Name="TargetDrive" Height="36" FontSize="14" Foreground="#172235"/></StackPanel></Grid><StackPanel Orientation="Horizontal" Margin="0,15,0,0"><Button x:Name="StartScan" Content="开始分析" Background="#347965"/><Button x:Name="CancelScan" Content="取消扫描" IsEnabled="False"/><Button x:Name="Export" Content="导出 JSON 报告" IsEnabled="False"/><Button x:Name="ShowPlans" Content="查看迁移策略 →"/></StackPanel></StackPanel></Border>
   <StackPanel Grid.Row="1" Margin="0,18,0,16"><TextBlock x:Name="ScanStatus" Text="尚未扫描 · 建议先分析 C 盘，或选择某个大目录" Foreground="#79DAC4" TextWrapping="Wrap"/><TextBlock x:Name="ScanCoverage" Text="仅扫描元数据；跳过联接与云占位；默认最多 30 万条目。" Foreground="#7B90AB" FontSize="11" Margin="0,6,0,0" TextWrapping="Wrap"/></StackPanel>
   <TabControl Grid.Row="2" Background="#141D2B" BorderBrush="#233047"><TabItem Header="目录排行"><DataGrid x:Name="DirectoryGrid"><DataGrid.Columns><DataGridTextColumn Header="目录（父子项存在重叠）" Binding="{Binding Path}" Width="*"/><DataGridTextColumn Header="逻辑长度" Binding="{Binding Size}" Width="110"/><DataGridTextColumn Header="分类" Binding="{Binding Category}" Width="140"/></DataGrid.Columns></DataGrid></TabItem><TabItem Header="大文件 TOP 30"><DataGrid x:Name="FilesGrid"><DataGrid.Columns><DataGridTextColumn Header="文件" Binding="{Binding Path}" Width="*"/><DataGridTextColumn Header="逻辑长度" Binding="{Binding Size}" Width="110"/><DataGridTextColumn Header="分类" Binding="{Binding Category}" Width="140"/></DataGrid.Columns></DataGrid></TabItem></TabControl>
  </Grid>
  <Grid x:Name="PlanPage" Grid.Row="1" Visibility="Collapsed"><Grid.RowDefinitions><RowDefinition Height="Auto"/><RowDefinition Height="*"/></Grid.RowDefinitions><Border Background="#302A1E" Padding="15" CornerRadius="8" Margin="0,0,0,16"><TextBlock Text="计划不执行搬移。容量是已扫描的逻辑长度，父子项重叠，不能相加；迁移完成并核验前不会释放 C 盘空间。" Foreground="#E2C894" TextWrapping="Wrap"/></Border><Grid Grid.Row="1"><Grid.ColumnDefinitions><ColumnDefinition Width="310"/><ColumnDefinition Width="*"/></Grid.ColumnDefinitions><ListBox x:Name="PlanList" Background="#141D2B" Foreground="#D6E1EF" BorderThickness="0" Padding="8" DisplayMemberPath="Label" ScrollViewer.HorizontalScrollBarVisibility="Disabled"/><Border Grid.Column="1" Background="{StaticResource Panel}" CornerRadius="8" Padding="20" Margin="16,0,0,0"><DockPanel><StackPanel DockPanel.Dock="Bottom"><Button x:Name="CopyPreview" Content="复制 Robocopy 预演命令" IsEnabled="False" HorizontalAlignment="Left" Margin="0,15,0,0"/><TextBlock Text="命令带 /L，仅列出文件；不会复制、删除或移动。" FontSize="11" Foreground="#7188A6" TextWrapping="Wrap" Margin="0,10,0,0"/></StackPanel><ScrollViewer VerticalScrollBarVisibility="Auto"><StackPanel><TextBlock x:Name="PlanTitle" Text="先分析，再选择迁移路线" FontSize="20" TextWrapping="Wrap"/><TextBlock x:Name="PlanSource" Foreground="#79DAC4" Margin="0,14,0,0" TextWrapping="Wrap"/><TextBlock x:Name="PlanSteps" Text="完成扫描后，这里会按实际目录生成个人文件、开发缓存、应用数据与系统组件的处理建议。" Foreground="#A1B2C9" TextWrapping="Wrap" LineHeight="24" Margin="0,18,0,0"/><TextBox x:Name="Preview" IsReadOnly="True" TextWrapping="Wrap" Margin="0,20,0,0" Visibility="Collapsed"/></StackPanel></ScrollViewer></DockPanel></Border></Grid></Grid>
  <ScrollViewer x:Name="InfoPage" Grid.Row="1" Visibility="Collapsed" VerticalScrollBarVisibility="Auto"><StackPanel><TextBlock Text="知道数字的边界，才能做对决定。" FontSize="20" Margin="0,0,0,20"/><TextBlock Foreground="#A1B2C9" TextWrapping="Wrap" LineHeight="26" Text="采集方式&#10;CPU 使用 GetSystemTimes 的 64 位差分；时间基准使用不含睡眠的 QueryUnbiasedInterruptTime。物理内存来自 GlobalMemoryStatusEx，提交量与系统缓存来自 GetPerformanceInfo。磁盘可用空间来自 GetDiskFreeSpaceExW。&#10;&#10;内存压力&#10;可用物理内存低于 10% / 5%，或提交量超过上限的 85% / 95%，提示关注 / 严重。这是产品启发式，不是 Windows 内核压力等级。提交量不等于分页文件已用量。温度、分页读写速率与磁盘 I/O 未采集；超过 64 个逻辑处理器时 CPU 仅反映当前处理器组。&#10;&#10;C 盘分析&#10;只读取文件系统元数据，不跟随重解析点、目录联接、离线文件或云占位。统计逻辑长度：压缩、稀疏文件、硬链接、卷影副本、NTFS 元数据可能造成与磁盘属性不同。不可读、超时与条目限制会明确标记；扫描数字不代表可安全释放的容量。&#10;&#10;迁移策略&#10;个人文件夹优先使用「属性 → 位置 → 移动」。缓存通过应用配置迁移。系统目录与已安装应用不直接剪切。Robocopy 只提供 /L 预演，目标盘需预留至少 10% 或 5 GiB；执行前仍需备份、核验实际空间和云同步状态。&#10;&#10;隐私与退出&#10;运行时不联网，不上传数据；扫描结果仅在内存中，点击导出才会保存。导出报告包含本机路径，分享前自行审查。界面脚本嵌入桌面程序；不安装服务，不添加开机启动。收起到托盘继续采样，关闭窗口或托盘「退出」结束程序。&#10;&#10;基于 corn12138/hostwatch（MIT）开发。Windows 桌面使用系统自带 WPF / Windows PowerShell 5.1，无浏览器内核。"/><Button x:Name="SourceLink" Content="查看上游项目" HorizontalAlignment="Left" Margin="0,22,0,0"/></StackPanel></ScrollViewer>
  <TextBlock x:Name="Footer" Grid.Row="2" Text="只读诊断 / 本地运行 / 5 秒刷新" Foreground="#5F7693" FontSize="11" Margin="0,18,0,0" TextWrapping="Wrap"/>
 </Grid>
</Grid></Window>
'@
try {
 $reader = [System.Xml.XmlNodeReader]::new($xaml)
 $script:Window = [Windows.Markup.XamlReader]::Load($reader)
 $script:Controls = @{}
 $namespaces = [System.Xml.XmlNamespaceManager]::new($xaml.NameTable)
 $namespaces.AddNamespace('x','http://schemas.microsoft.com/winfx/2006/xaml')
 $xaml.SelectNodes('//*[@x:Name]', $namespaces) | ForEach-Object { $name=$_.GetAttribute('Name','http://schemas.microsoft.com/winfx/2006/xaml'); $script:Controls[$name] = $Window.FindName($name) }
} catch {
 [System.Windows.MessageBox]::Show($_.Exception.ToString(),'Hostwatch 启动失败') | Out-Null
 exit 1
}
function Show-Page([string]$Page) {
 foreach ($p in @('Home','Scan','Plan','Info')) { $Controls[$p+'Page'].Visibility = if ($p -eq $Page) { 'Visible' } else { 'Collapsed' }; $Controls['Nav'+$p].Background = if($p -eq $Page) { '#25473F' } else { '#223149' } }
 $Controls.PageTitle.Text = @{Home='资源总览';Scan='C 盘分析';Plan='迁移策略';Info='数据说明'}[$Page]
 $Controls.PageSubtitle.Text = @{Home='看清当前负载，再决定是否需要处理。';Scan='从占用结构出发，找到值得处理的目录。';Plan='按实际占用，规划下一步。';Info='指标来源、扫描覆盖与操作边界。'}[$Page]
}
function Apply-Snapshot($Report) {
 $s = $Report.snapshot; $w = $s.windows
 $Controls.LiveTime.Text = '● 本机 · ' + (Get-Date -Format 'HH:mm:ss')
 $Controls.CpuValue.Text = if ($null -eq $s.activity.cpu_busy_percent) { '—' } else { '{0:N0}%' -f $s.activity.cpu_busy_percent }
 $Controls.CpuBar.Value = if ($null -eq $s.activity.cpu_busy_percent) { 0 } else { $s.activity.cpu_busy_percent }
 $percent = 100 * $w.used_physical_bytes / $s.memory.total_bytes
 $Controls.MemValue.Text = '{0:N0}%' -f $percent; $Controls.MemBar.Value = $percent
 $Controls.MemDetail.Text = (Size $w.used_physical_bytes) + ' / ' + (Size $s.memory.total_bytes)
 if ($null -ne $s.disk) { $Controls.DiskValue.Text = Size $s.disk.available_bytes; $Controls.DiskDetail.Text = '总容量 ' + (Size $s.disk.total_bytes); $Controls.DiskBar.Value = 100*$s.disk.available_bytes/$s.disk.total_bytes }
 else { $Controls.DiskValue.Text='—'; $Controls.DiskDetail.Text='本次无法读取'; $Controls.DiskBar.Value=0 }
 if ($null -ne $w.commit_used_bytes -and $null -ne $w.commit_limit_bytes) { $Controls.CommitValue.Text = Size $w.commit_used_bytes; $Controls.CommitDetail.Text = '上限 ' + (Size $w.commit_limit_bytes); $Controls.CommitBar.Value = 100*$w.commit_used_bytes/[Math]::Max([double]1,[double]$w.commit_limit_bytes) }
 else { $Controls.CommitValue.Text='—'; $Controls.CommitDetail.Text='本次无法读取'; $Controls.CommitBar.Value=0 }
 $rows = @($s.top_processes | ForEach-Object { [pscustomobject]@{Name=$_.name;Pid=$_.pid;Size=(Size $_.resident_bytes)} }); $Controls.ProcessGrid.ItemsSource = $rows
 $issues = @($Report.advice | Where-Object { $_.severity -ne 'info' })
 $Controls.HealthTitle.Text = if ($issues.Count -gt 0) { '有 ' + $issues.Count + ' 项值得关注' } else { '当前已测指标无需处理' }
 $Controls.HealthDetail.Text = if ($issues.Count -gt 0) { ($issues | ForEach-Object { $_.evidence + '。' + $_.action }) -join "`n" } else { '物理内存、CPU 与 C 盘容量已更新。温度、分页读写与磁盘 I/O 未采集；无需为了降低内存数字而清缓存。' }
 $failed = @($s.sources | Where-Object { -not $_.ok })
 if ($failed.Count) { $Controls.Footer.Text = '部分采集失败：' + (($failed | ForEach-Object { $_.source + ' ' + $_.error }) -join '；') }
 else { $Controls.Footer.Text = '只读诊断 / 本地运行 / 5 秒刷新 / 内存压力采用可用量与提交余量启发式' }
}
function Apply-Scan($Report) {
 $script:ScanReport = $Report
 $Controls.ScanStatus.Text = $Report.stop_reason + ' · ' + ('{0:N1}s' -f $Report.elapsed_seconds) + ' · ' + (Size $Report.logical_bytes) + ' 已读逻辑长度'
 $Controls.ScanCoverage.Text = '文件 ' + $Report.files + ' · 条目 ' + $Report.entries_visited + ' · 跳过云占位/联接 ' + $Report.skipped_reparse_or_cloud + ' · 不可读 ' + $Report.unreadable_entries + ' · 父子目录容量不能相加'
 $Controls.DirectoryGrid.ItemsSource = @($Report.directories | ForEach-Object { [pscustomobject]@{Path=$_.path;Size=(Size $_.bytes);Category=$_.category} })
 $Controls.FilesGrid.ItemsSource = @($Report.largest_files | ForEach-Object { [pscustomobject]@{Path=$_.path;Size=(Size $_.bytes);Category=$_.category} })
 $Controls.PlanList.ItemsSource = @($Report.strategies | ForEach-Object { [pscustomobject]@{Label=($_.risk + ' · ' + (Size $_.measured_bytes) + "`n" + $_.source);Data=$_} })
 $Controls.Export.IsEnabled = $true
 if ($Controls.PlanList.Items.Count -gt 0) { $Controls.PlanList.SelectedIndex = 0 }
 else { $Controls.PlanTitle.Text = '当前扫描未发现匹配策略的目录'; $Controls.PlanSteps.Text = '可扩大扫描范围，或针对 Users、Downloads、AppData 等目录重新扫描。'; $Controls.PlanSource.Text=''; $Controls.Preview.Visibility='Collapsed'; $Controls.CopyPreview.IsEnabled=$false }
}
$Controls.NavHome.Add_Click({ Show-Page Home }); $Controls.NavScan.Add_Click({ Show-Page Scan }); $Controls.NavPlan.Add_Click({ Show-Page Plan }); $Controls.NavInfo.Add_Click({ Show-Page Info })
$Controls.QuickScan.Add_Click({ Show-Page Scan }); $Controls.ShowPlans.Add_Click({ Show-Page Plan })
$Controls.StorageSettings.Add_Click({ Start-Process 'ms-settings:storagesense' }); $Controls.TaskManager.Add_Click({ Start-Process taskmgr.exe })
$Controls.SourceLink.Add_Click({ Start-Process 'https://github.com/corn12138/hostwatch' })
$Controls.TargetDrive.Items.Add('自动选择') | Out-Null
[System.IO.DriveInfo]::GetDrives() | Where-Object { $_.IsReady -and $_.DriveType -eq 'Fixed' -and $_.Name -ne 'C:\' } | ForEach-Object { $Controls.TargetDrive.Items.Add($_.Name) | Out-Null }; $Controls.TargetDrive.SelectedIndex=0
$Controls.StartScan.Add_Click({
 try {
  $seconds = 0
  if (-not [int]::TryParse($Controls.ScanSeconds.Text,[ref]$seconds) -or $seconds -lt 1 -or $seconds -gt 3600) { throw '扫描时限需要 1–3600 秒整数。' }
  $scanArgs=@('analyze','--path',$Controls.ScanRoot.Text,'--seconds',"$seconds",'--json')
  if ($Controls.TargetDrive.SelectedIndex -gt 0) { $scanArgs+=@('--target',[string]$Controls.TargetDrive.SelectedItem) }
  Start-Backend 'scan' $scanArgs
  $Controls.StartScan.IsEnabled=$false; $Controls.CancelScan.IsEnabled=$true
  $Controls.ScanStatus.Text='正在扫描元数据… 0 秒'; $Controls.ScanCoverage.Text='扫描有时间与条目上限，可以随时取消；历史结果保留到新扫描成功。'
 } catch { $Controls.ScanStatus.Text='无法开始扫描：' + $_.Exception.Message }
})
$Controls.CancelScan.Add_Click({
 if ($script:Jobs.ContainsKey('scan')) { $j=$script:Jobs['scan']; if(-not $j.Process.HasExited){$j.Process.Kill()}; $j.Process.WaitForExit(); $j.Process.Dispose(); $script:Jobs.Remove('scan') }
 $Controls.StartScan.IsEnabled=$true; $Controls.CancelScan.IsEnabled=$false; $Controls.ScanStatus.Text='本次扫描已取消；此前完成的报告仍可查看。'
})
$Controls.PlanList.Add_SelectionChanged({
 if ($null -eq $Controls.PlanList.SelectedItem) { return }; $p=$Controls.PlanList.SelectedItem.Data
 $Controls.PlanTitle.Text=$p.title + ' · ' + $p.risk
 $Controls.PlanSource.Text=$p.source + "`n已读逻辑长度 " + (Size $p.measured_bytes)
 if ($p.destination) { $Controls.PlanSource.Text+="`n建议目标：" + $p.destination }
 $Controls.PlanSteps.Text=($p.steps | ForEach-Object -Begin {$n=0} -Process { $n++; "$n. $_`n" }) -join "`n"
 $Controls.CopyPreview.IsEnabled=[bool]$p.preview_command
 $Controls.Preview.Visibility=if($p.preview_command){'Visible'}else{'Collapsed'}; $Controls.Preview.Text=[string]$p.preview_command
})
$Controls.CopyPreview.Add_Click({ [System.Windows.Clipboard]::SetText($Controls.Preview.Text); $Controls.Footer.Text='已复制预演命令；在 PowerShell 中查看文件列表，不会搬移文件。' })
$Controls.Export.Add_Click({
 if($null -eq $script:ScanReport){return}
 $dialog=[Microsoft.Win32.SaveFileDialog]::new();$dialog.Filter='JSON 报告 (*.json)|*.json';$dialog.FileName='hostwatch-storage-' + (Get-Date -Format yyyyMMdd-HHmmss) + '.json'
 if($dialog.ShowDialog($Window)) { try { [System.IO.File]::WriteAllText($dialog.FileName,($script:ScanReport | ConvertTo-Json -Depth 20),[System.Text.UTF8Encoding]::new($false)); $Controls.Footer.Text='报告已保存：' + $dialog.FileName } catch { $Controls.Footer.Text='导出失败：' + $_.Exception.Message } }
})
$script:Tray=[System.Windows.Forms.NotifyIcon]::new();$Tray.Icon=[System.Drawing.SystemIcons]::Application;$Tray.Text='Hostwatch · Windows';$Tray.Visible=$true
$menu=[System.Windows.Forms.ContextMenuStrip]::new();[void]$menu.Items.Add('打开面板',$null,{ $Window.Show();$Window.WindowState='Normal';$Window.Activate() | Out-Null });[void]$menu.Items.Add('退出',$null,{ $Window.Close() });$Tray.ContextMenuStrip=$menu
$Tray.Add_DoubleClick({$Window.Show();$Window.Activate() | Out-Null});$Controls.ToTray.Add_Click({$Window.Hide()})
$script:LastSample=[DateTime]::MinValue
$script:Timer=[Windows.Threading.DispatcherTimer]::new();$Timer.Interval=[TimeSpan]::FromMilliseconds(250)
$Timer.Add_Tick({
 try {
  foreach($kind in @($script:Jobs.Keys)) {
   $j=$script:Jobs[$kind]
   if(-not $j.Process.HasExited) { if($kind -eq 'scan') { $Controls.ScanStatus.Text='正在扫描元数据… ' + [int]([DateTime]::Now-$j.Started).TotalSeconds + ' 秒' };continue }
   if(-not $j.Out.IsCompleted -or -not $j.Err.IsCompleted){continue}
   $exitCode=$j.Process.ExitCode;$output=$j.Out.Result;$errorText=$j.Err.Result;$j.Process.Dispose();$script:Jobs.Remove($kind)
   if($kind -eq 'scan'){$Controls.StartScan.IsEnabled=$true;$Controls.CancelScan.IsEnabled=$false}
   if($exitCode -ne 0){if($kind -eq 'scan'){$Controls.ScanStatus.Text='扫描失败：'+$errorText}else{throw $errorText};continue}
   $report=$output | ConvertFrom-Json
   if($kind -eq 'scan'){Apply-Scan $report}else{Apply-Snapshot $report}
  }
  if(([DateTime]::Now-$script:LastSample).TotalSeconds -ge 5 -and -not $script:Jobs.ContainsKey('snapshot')) { Start-Backend 'snapshot' @('snapshot','--json','--processes');$script:LastSample=[DateTime]::Now }
 } catch { $Controls.LiveTime.Text='● 采集暂不可用';$Controls.Footer.Text=$_.Exception.Message;$Controls.CpuValue.Text='—';$Controls.MemValue.Text='—';$Controls.DiskValue.Text='—';$Controls.CommitValue.Text='—';$Controls.HealthTitle.Text='采集失败，等待下次重试';$Controls.HealthDetail.Text='旧值不代表当前状态。'+$_.Exception.Message }
})
$Window.Add_Closed({ $script:Closing=$true;$Timer.Stop();$Tray.Visible=$false;$Tray.Dispose();foreach($j in $script:Jobs.Values){if(-not $j.Process.HasExited){$j.Process.Kill()};$j.Process.Dispose()} })
try { $Timer.Start();[void]$Window.ShowDialog() } finally { $Timer.Stop();$Tray.Dispose() }
