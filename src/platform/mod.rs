use crate::model::Snapshot;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::scan_processes;

pub trait Collector {
    fn collect(&mut self, processes: bool) -> Result<Snapshot, String>;
}

pub fn collector() -> Result<Box<dyn Collector>, String> {
    #[cfg(target_os = "macos")]
    return Ok(Box::new(macos::MacCollector::new()));

    #[cfg(target_os = "windows")]
    return windows::collector();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Err("当前仅实现 macOS 采集；Windows 接口已预留。".into())
}
