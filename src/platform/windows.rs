use super::Collector;

pub fn collector() -> Result<Box<dyn Collector>, String> {
    Err("Windows 适配尚未实现；不会返回模拟指标。".into())
}
