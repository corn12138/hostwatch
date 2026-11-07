# Headless WPF component tests. No window is shown and no user input is injected.
param([Parameter(Mandatory=$true)][string]$Backend)
$ErrorActionPreference = 'Stop'
$source = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'dashboard.ps1'))
$tokens = $null; $parseErrors = $null
[System.Management.Automation.Language.Parser]::ParseInput($source,[ref]$tokens,[ref]$parseErrors) | Out-Null
if($parseErrors.Count) { throw ($parseErrors | Out-String) }
$prefix = $source.Substring(0,$source.IndexOf('$script:Tray='))
. ([scriptblock]::Create($prefix)) -Backend $Backend
$testBase = Join-Path ([System.IO.Path]::GetTempPath()) 'hostwatch-component-tests'
$fixture = Join-Path $testBase ([guid]::NewGuid().ToString())
try {
 $snapshot = & $Backend snapshot --json --processes
 if($LASTEXITCODE){throw 'snapshot failed'}
 Apply-Snapshot ($snapshot | ConvertFrom-Json)
 if($Controls.MemBar.Value -le 0 -or $Controls.ProcessGrid.Items.Count -eq 0){throw 'snapshot bindings failed'}
 foreach($route in @('Home','Scan','Plan','Info')) {
  $Controls['Nav'+$route].RaiseEvent([System.Windows.RoutedEventArgs]::new([System.Windows.Controls.Button]::ClickEvent))
  if($Controls[$route+'Page'].Visibility -ne 'Visible'){throw "navigation failed: $route"}
 }
 [void][System.IO.Directory]::CreateDirectory((Join-Path $fixture 'node_modules'))
 [System.IO.File]::WriteAllText((Join-Path $fixture 'node_modules/package.json'),'{}')
 Start-Backend 'scan' @('analyze','--path',(Join-Path $fixture 'node_modules'),'--json')
 $job=$script:Jobs['scan']
 if(-not $job.Process.WaitForExit(10000)){throw 'scan timed out'}
 if($job.Process.ExitCode){throw $job.Err.Result}
 $scan=$job.Out.Result | ConvertFrom-Json
 Apply-Scan $scan
 $job.Process.Dispose();$script:Jobs.Remove('scan')
 if($scan.logical_bytes -ne 2 -or $Controls.PlanList.Items.Count -eq 0 -or -not $Controls.Export.IsEnabled){throw 'scan / plan bindings failed'}
 $roundTrip=$script:ScanReport | ConvertTo-Json -Depth 20 | ConvertFrom-Json
 if($roundTrip.logical_bytes -ne 2){throw 'export serialization failed'}
 Start-Backend 'scan' @('watch','--interval','5','--count','10','--json')
 $Controls.CancelScan.RaiseEvent([System.Windows.RoutedEventArgs]::new([System.Windows.Controls.Button]::ClickEvent))
 if($script:Jobs.ContainsKey('scan')){throw 'cancellation failed'}
 Write-Output 'WPF components passed: parsing, real metrics, navigation, asynchronous scan, strategy binding, export serialization, cancellation.'
} finally {
 foreach($job in $script:Jobs.Values){if(-not $job.Process.HasExited){$job.Process.Kill()};$job.Process.Dispose()}
 $Window.Close()
 $resolved=[System.IO.Path]::GetFullPath($fixture)
 if(-not $resolved.StartsWith([System.IO.Path]::GetFullPath($testBase)+[System.IO.Path]::DirectorySeparatorChar)){throw 'fixture cleanup path escaped test directory'}
 if(Test-Path -LiteralPath $resolved){Remove-Item -LiteralPath $resolved -Recurse -Force}
}
