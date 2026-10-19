use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pressure {
    Normal,
    Warning,
    Critical,
    Unknown,
}

/// Activity Monitor-style breakdown derived from kernel page counts; values are estimates.
#[derive(Clone, Debug, Serialize)]
pub struct MemoryBreakdown {
    pub used_bytes: u64,
    pub app_bytes: u64,
    pub wired_bytes: u64,
    pub compressed_bytes: u64,
    pub cached_files_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Swap {
    pub total_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct Memory {
    pub total_bytes: u64,
    /// None when the VM statistics source failed in this sample; never reused from earlier samples.
    pub breakdown: Option<MemoryBreakdown>,
    pub swap: Option<Swap>,
    pub pressure: Pressure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RateStatus {
    /// Rates cover the interval since the previous valid sample.
    Ready,
    /// No previous sample yet.
    Warmup,
    /// A cumulative counter went backwards (reboot, kernel reset or source change).
    CounterReset,
    /// Wall-clock time advanced far beyond monotonic time: the machine slept or the clock jumped.
    SleepGap,
    /// Counter source unavailable in this or the previous sample.
    Unavailable,
}

#[derive(Clone, Debug, Serialize)]
pub struct Activity {
    pub status: RateStatus,
    pub interval_seconds: Option<f64>,
    pub swap_in_bytes_per_second: Option<f64>,
    pub swap_out_bytes_per_second: Option<f64>,
    pub page_in_bytes_per_second: Option<f64>,
    pub cpu_busy_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Disk {
    pub path: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub resident_bytes: u64,
}

/// Outcome of one data source in one sample, so consumers can tell missing from healthy.
#[derive(Debug, Serialize)]
pub struct SourceStatus {
    pub source: &'static str,
    pub ok: bool,
    pub elapsed_micros: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub unix_seconds: u64,
    pub platform: &'static str,
    pub memory: Memory,
    pub activity: Activity,
    pub disk: Option<Disk>,
    pub processes_requested: bool,
    pub top_processes: Vec<Process>,
    pub sources: Vec<SourceStatus>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Serialize)]
pub struct Advice {
    pub id: &'static str,
    pub severity: Severity,
    pub evidence: String,
    pub action: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Raised,
    Escalated,
    Recovered,
}

#[derive(Debug, Serialize)]
pub struct Event {
    pub id: &'static str,
    pub kind: EventKind,
    pub severity: Severity,
    pub evidence: String,
    pub action: &'static str,
    /// Consecutive valid samples supporting this event.
    pub streak: u32,
}

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub snapshot: Snapshot,
    pub advice: Vec<Advice>,
    /// Sustained-state transitions; only produced by `watch`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<Event>>,
}
