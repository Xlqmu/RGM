use crate::data::{GpuData, GpuInfo, ProcessInfo};
use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::{Clock, PcieUtilCounter, TemperatureSensor};
use nvml_wrapper::enums::device::UsedGpuMemory;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MonitorError {
    #[error("NVML initialization failed: {0}")]
    NvmlInit(#[from] nvml_wrapper::error::NvmlError),
    #[error("Device not found at index {0}")]
    DeviceNotFound(u32),
    #[error("Failed to get data: {0}")]
    SamplingFailed(String),
}

pub trait GpuMonitor: Send + Sync {
    fn get_static_info(&self) -> GpuInfo;
    fn sample(&self) -> Result<(GpuData, Vec<ProcessInfo>), MonitorError>;
}

pub struct NvmlMonitor {
    nvml: Nvml,
    device_index: u32,
    start_time: std::time::Instant,
}

impl NvmlMonitor {
    pub fn new(device_index: u32) -> Result<Self, MonitorError> {
        let nvml = Nvml::init()?;
        // 在创建时验证设备是否存在，以提前抛出错误
        nvml.device_by_index(device_index)?;
        Ok(Self {
            nvml,
            device_index,
            start_time: std::time::Instant::now(),
        })
    }
}

impl GpuMonitor for NvmlMonitor {
    fn get_static_info(&self) -> GpuInfo {
        // 在需要时临时获取 device 对象
        let device = self.nvml.device_by_index(self.device_index).unwrap();

        GpuInfo {
            name: device.name().unwrap_or_else(|_| "N/A".to_string()),
            uuid: device.uuid().unwrap_or_else(|_| "N/A".to_string()),
            driver_version: self
                .nvml
                .sys_driver_version()
                .unwrap_or_else(|_| "N/A".to_string()),
            vbios_version: device.vbios_version().unwrap_or_else(|_| "N/A".to_string()),
            pcie_gen: device.current_pcie_link_gen().unwrap_or(0),
            pcie_width: device.current_pcie_link_width().unwrap_or(0),
        }
    }

    fn sample(&self) -> Result<(GpuData, Vec<ProcessInfo>), MonitorError> {
        // 在需要时临时获取 device 对象
        let device = self.nvml.device_by_index(self.device_index)?;

        let (util, mem, temp) = (
            device.utilization_rates()?,
            device.memory_info()?,
            device.temperature(TemperatureSensor::Gpu)?,
        );

        let gpu_clock = device.clock_info(Clock::Graphics).unwrap_or(0);
        let mem_clock = device.clock_info(Clock::Memory).unwrap_or(0);

        let (power_usage, power_limit) =
            match (device.power_usage(), device.power_management_limit()) {
                (Ok(usage), Ok(limit)) => (usage as f64 / 1000.0, limit as f64 / 1000.0),
                _ => (0.0, 0.0),
            };

        let fan_speed = device.fan_speed(0).unwrap_or(0);

        let (pcie_tx, pcie_rx) = match (
            device.pcie_throughput(PcieUtilCounter::Send),
            device.pcie_throughput(PcieUtilCounter::Receive),
        ) {
            (Ok(rx), Ok(tx)) => (tx as f64 / 1024.0, rx as f64 / 1024.0),
            _ => (0.0, 0.0),
        };

        let gpu_data = GpuData {
            timestamp: self.start_time.elapsed().as_secs_f64(),
            utilization: util.gpu as f32,
            memory_used: mem.used as f64 / 1024.0 / 1024.0 / 1024.0,
            memory_total: mem.total as f64 / 1024.0 / 1024.0 / 1024.0,
            temperature: temp,
            gpu_clock,
            memory_clock: mem_clock,
            power_usage,
            power_limit,
            fan_speed,
            pcie_throughput_tx: pcie_tx,
            pcie_throughput_rx: pcie_rx,
        };

        let mut process_infos = Vec::new();
        if let Ok(procs) = device.running_graphics_processes() {
            for proc in procs {
                let proc_name = std::fs::read_to_string(format!("/proc/{}/comm", proc.pid))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                let memory_usage = match proc.used_gpu_memory {
                    UsedGpuMemory::Used(v) => v,
                    _ => 0,
                };
                process_infos.push(ProcessInfo {
                    pid: proc.pid,
                    name: proc_name,
                    memory_usage,
                    cpu_percent: 0.0,
                });
            }
        }

        Ok((gpu_data, process_infos))
    }
}

pub fn create_monitor() -> Option<Box<dyn GpuMonitor>> {
    if let Ok(monitor) = NvmlMonitor::new(0) {
        println!("✅ NVML monitor initialized successfully.");
        return Some(Box::new(monitor));
    }
    println!("❌ No compatible GPU monitors found.");
    None
}
