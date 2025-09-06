// GPU data structure, storing dynamic information
#[derive(Clone, Debug)]
pub struct GpuData {
    pub timestamp: f64,
    pub utilization: f32,
    pub memory_used: f64,
    pub memory_total: f64,
    pub temperature: u32,
    pub gpu_clock: u32,
    pub memory_clock: u32,
    pub power_usage: f64,
    pub power_limit: f64,
    pub fan_speed: u32,
    pub pcie_throughput_tx: f64,
    pub pcie_throughput_rx: f64,
}

// GPU information structure, storing static information
#[derive(Clone, Debug, Default)]
pub struct GpuInfo {
    pub name: String,
    pub uuid: String,
    pub pcie_gen: u32,
    pub pcie_width: u32,
    pub driver_version: String,
    pub vbios_version: String,
}

// Process information structure, storing information about GPU processes
#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub memory_usage: u64,
    #[allow(dead_code)]
    pub cpu_percent: f32,
}
