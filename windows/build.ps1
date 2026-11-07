# Windows PowerShell 5.1 / PowerShell 7. Run from a VS Developer shell, or use the local SDK fallback.
param([switch]$Check)
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
Push-Location $project
try {
 if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
  $sdk = Join-Path $project '.local/toolchain/sdk'
  if (Test-Path -LiteralPath $sdk) {
   $sysroot = (& rustc --print sysroot).Trim()
   $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = Join-Path $sysroot 'lib/rustlib/x86_64-pc-windows-msvc/bin/rust-lld.exe'
   $env:LIB = "$sdk/crt/lib/x86_64;$sdk/sdk/lib/ucrt/x86_64;$sdk/sdk/lib/um/x86_64"
  }
 }
 if ($Check) {
  & cargo fmt --all -- --check; if($LASTEXITCODE){throw 'rustfmt failed'}
  & cargo clippy --locked --all-targets -- -D warnings; if($LASTEXITCODE){throw 'clippy failed'}
  & cargo test --locked; if($LASTEXITCODE){throw 'tests failed'}
  & (Join-Path $PSScriptRoot 'test-dashboard.ps1') -Backend (Join-Path $project 'target/debug/hostwatch.exe')
 }
 & cargo build --locked --release --bins; if($LASTEXITCODE){throw 'release build failed'}
 $output = Join-Path $project 'dist/Hostwatch-Windows-0.4.0'
 New-Item -ItemType Directory -Path $output -Force | Out-Null
 Copy-Item -LiteralPath 'target/release/hostwatch.exe' -Destination $output
 $compiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
 $automation = Get-ChildItem -LiteralPath (Join-Path $env:WINDIR 'Microsoft.NET/assembly/GAC_MSIL/System.Management.Automation') -Filter System.Management.Automation.dll -Recurse | Select-Object -First 1 -ExpandProperty FullName
 if (-not $automation) { throw 'Windows PowerShell 5.1 runtime was not found' }
 $compilerArgs = @('/nologo','/target:winexe','/platform:x64','/optimize+',("/out:$output/Hostwatch-Windows.exe"),("/reference:$automation"),'/reference:System.Windows.Forms.dll',('/resource:'+(Join-Path $project 'windows/dashboard.ps1')+',dashboard.ps1'),(Join-Path $project 'windows/HostwatchDesktop.cs'))
 & $compiler @compilerArgs
 if($LASTEXITCODE){throw 'native desktop build failed'}
 Copy-Item -LiteralPath 'LICENSE' -Destination $output
 Copy-Item -LiteralPath 'windows/README.zh-CN.md' -Destination $output
 Compress-Archive -Path $output -DestinationPath (Join-Path $project 'dist/Hostwatch-0.4.0-windows-x64.zip') -Force
 Write-Output "Portable application: $output/Hostwatch-Windows.exe"
} finally { Pop-Location }
