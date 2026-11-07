//! Native WPF shell, shipped inside the executable; no web server or browser runtime.
#[cfg(windows)]
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::{os::windows::process::CommandExt, process::Command};
    let executable = std::env::current_exe()?;
    let shell = executable
        .parent()
        .ok_or("无法定位程序目录")?
        .join("Hostwatch-Windows.exe");
    if !shell.is_file() {
        return Err(
            "请先运行 windows/build.ps1 打包桌面版，再打开 dist 中的 Hostwatch-Windows.exe".into(),
        );
    }
    let result = Command::new(shell).creation_flags(0x08000000).status();
    if !result?.success() {
        return Err("桌面面板启动失败，请在终端运行 hostwatch desktop 查看错误".into());
    }
    Ok(())
}
