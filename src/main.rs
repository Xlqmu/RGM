use crossbeam_channel::{Receiver, bounded};
use eframe::egui;
use egui::{Color32, ViewportBuilder};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use nvml_wrapper::{
    Nvml,
    enum_wrappers::device::{Clock, PcieUtilCounter, TemperatureSensor},
    enums::device::UsedGpuMemory,
};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::{sync::Arc, thread, time::Duration};

// GPU 数据结构
#[derive(Clone, Debug)]
struct GpuData {
    timestamp: f64,          // 时间戳（秒）
    utilization: f32,        // GPU 利用率 (%)
    memory_used: f64,        // 已用显存 (GB)
    memory_total: f64,       // 总显存 (GB)
    temperature: u32,        // 温度 (°C)
    gpu_clock: u32,          // GPU 时钟频率 (MHz)
    memory_clock: u32,       // 内存时钟频率 (MHz)
    power_usage: f64,        // 功率使用 (W)
    power_limit: f64,        // 功率限制 (W)
    fan_speed: u32,          // 风扇转速 (%)
    pcie_throughput_tx: f64, // PCIe 传输速率 (MB/s)
    pcie_throughput_rx: f64, // PCIe 接收速率 (MB/s)
}

// GPU 信息结构体，存储静态信息
#[allow(dead_code)]
struct GpuInfo {
    name: String,           // GPU 名称
    uuid: String,           // GPU UUID
    pcie_gen: u32,          // PCIe 代数
    pcie_width: u32,        // PCIe 带宽
    driver_version: String, // 驱动版本
    vbios_version: String,  // VBIOS 版本
}

// 进程信息结构体
#[derive(Clone, Debug)]
struct ProcessInfo {
    pid: u32,          // 进程 ID
    name: String,      // 进程名称
    memory_usage: u64, // 显存使用量
    #[allow(dead_code)]
    cpu_percent: f32, // CPU 使用率
}

// 应用程序状态
struct RgmApp {
    data: Arc<Mutex<VecDeque<GpuData>>>,             // 历史数据
    receiver: Receiver<(GpuData, Vec<ProcessInfo>)>, // 接收器
    _start_time: std::time::Instant,                 // 程序开始时间
    #[allow(dead_code)]
    max_data_points: usize,   // 保留的最大数据点数
    display_duration: f64,                           // 显示的时间范围（秒）
    #[allow(dead_code)]
    gpu_info: Option<GpuInfo>, // GPU 静态信息
    processes: Arc<Mutex<Vec<ProcessInfo>>>,         // GPU 相关进程
}

impl RgmApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (sender, receiver) = bounded(100);
        let data = Arc::new(Mutex::new(VecDeque::with_capacity(120)));
        let data_clone = Arc::clone(&data);
        let processes = Arc::new(Mutex::new(Vec::new()));
        let processes_clone = Arc::clone(&processes);
        let gpu_info = None;

        thread::spawn(move || {
            let nvml = match Nvml::init() {
                Ok(nvml) => nvml,
                Err(err) => {
                    eprintln!("Failed to initialize NVML: {}", err);
                    return;
                }
            };
            let device = match nvml.device_by_index(0) {
                Ok(device) => device,
                Err(err) => {
                    eprintln!("Failed to get GPU device: {}", err);
                    return;
                }
            };
            let start_time = std::time::Instant::now();
            loop {
                let (util, mem, temp) = match (
                    device.utilization_rates(),
                    device.memory_info(),
                    device.temperature(TemperatureSensor::Gpu),
                ) {
                    (Ok(util), Ok(mem), Ok(temp)) => (util, mem, temp),
                    _ => {
                        eprintln!("Failed to get basic GPU information");
                        thread::sleep(Duration::from_millis(200));
                        continue;
                    }
                };

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
                    timestamp: start_time.elapsed().as_secs_f64(),
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

                let mut process_infos: Vec<ProcessInfo> = Vec::new();
                if let Ok(graphics_processes) = device.running_graphics_processes() {
                    for proc in graphics_processes {
                        if !process_infos.iter().any(|p| p.pid == proc.pid) {
                            let proc_name =
                                match std::fs::read_to_string(format!("/proc/{}/comm", proc.pid)) {
                                    Ok(name) => name.trim().to_string(),
                                    Err(_) => "unknown".to_string(),
                                };
                            let memory_usage = match proc.used_gpu_memory {
                                UsedGpuMemory::Used(v) => v,
                                UsedGpuMemory::Unavailable => 0,
                            };
                            process_infos.push(ProcessInfo {
                                pid: proc.pid,
                                name: proc_name,
                                memory_usage,
                                cpu_percent: 0.0,
                            });
                        }
                    }
                }
                if sender.send((gpu_data, process_infos)).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        });

        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.dark_mode = true;
        cc.egui_ctx.set_style(style);

        Self {
            data: data_clone,
            receiver,
            _start_time: std::time::Instant::now(),
            max_data_points: 120,
            display_duration: 10.0,
            gpu_info,
            processes: processes_clone,
        }
    }
}

impl eframe::App for RgmApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok((gpu_data, proc_infos)) = self.receiver.try_recv() {
            let mut data = self.data.lock().unwrap();
            let now = gpu_data.timestamp;
            let window_start_time = (now - self.display_duration).max(0.0);
            data.push_back(gpu_data);
            while data
                .front()
                .map_or(false, |d| d.timestamp < window_start_time)
            {
                data.pop_front();
            }
            let mut processes = self.processes.lock().unwrap();
            *processes = proc_infos;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("🚀 GPU Monitor");
            ui.add_space(8.0);

            let data_guard = self.data.lock().unwrap();
            let latest = data_guard.back();

            if let Some(latest) = latest {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "GPU Utilization: {}%",
                                    latest.utilization
                                ))
                                .color(Color32::GREEN)
                                .size(22.0)
                                .strong(),
                            );
                            ui.label(format!("Temperature: {}°C", latest.temperature));
                            ui.label(format!("Fan Speed: {}%", latest.fan_speed));
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.label(format!(
                                "Memory: {:.2}/{:.2} GB",
                                latest.memory_used, latest.memory_total
                            ));
                            ui.label(format!(
                                "Power: {:.2}/{:.2} W",
                                latest.power_usage, latest.power_limit
                            ));
                            ui.label(format!("GPU Clock: {} MHz", latest.gpu_clock));
                            ui.label(format!("Memory Clock: {} MHz", latest.memory_clock));
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.label(format!("PCIe TX: {:.2} MB/s", latest.pcie_throughput_tx));
                            ui.label(format!("PCIe RX: {:.2} MB/s", latest.pcie_throughput_rx));
                        });
                    });
                });
            }

            ui.add_space(12.0);
            ui.separator();

            // ==================== 逻辑修改开始 ====================
            ui.heading("📈 Real-time GPU Metrics (Last 10 Seconds)");

            let latest_timestamp = data_guard.back().map_or(0.0, |d| d.timestamp);

            // 将绝对时间戳转换为 [0, 10] 区间的相对时间
            // x = 最新时间 - 当前数据时间
            // 这样最新的数据点 x=0 (0秒前), 最老的数据点 x 接近 10 (10秒前)
            let to_relative_points = |mapper: Box<dyn Fn(&GpuData) -> f64>| -> PlotPoints {
                data_guard
                    .iter()
                    .map(|data| {
                        let x = latest_timestamp - data.timestamp;
                        [x.max(0.0), mapper(data)]
                    })
                    .collect()
            };

            let gpu_util_points: PlotPoints =
                to_relative_points(Box::new(|d| d.utilization as f64));
            let memory_points: PlotPoints =
                to_relative_points(Box::new(|d| d.memory_used / d.memory_total * 100.0));
            let temp_points: PlotPoints = to_relative_points(Box::new(|d| d.temperature as f64));

            let power_points: PlotPoints = data_guard
                .iter()
                .filter(|data| data.power_limit > 0.0)
                .map(|data| {
                    let x = latest_timestamp - data.timestamp;
                    [x.max(0.0), data.power_usage / data.power_limit * 100.0]
                })
                .collect();

            Plot::new("gpu_metrics_plot")
                .view_aspect(2.5)
                .legend(Legend::default())
                .include_y(0.0)
                .include_y(100.0)
                .include_x(0.0)
                .include_x(self.display_duration)
                .x_axis_label("Seconds Ago (0 = now)") // X轴标签已修改
                .show_x(true)
                .show_y(true)
                .show(ui, |plot_ui| {
                    plot_ui
                        .line(Line::new("GPU Utilization", gpu_util_points).color(Color32::GREEN));
                    plot_ui.line(
                        Line::new("Memory Usage (%)", memory_points)
                            .color(Color32::from_rgb(0, 128, 255)),
                    );
                    plot_ui.line(
                        Line::new("Temperature (°C)", temp_points)
                            .color(Color32::from_rgb(255, 128, 0)),
                    );
                    plot_ui.line(
                        Line::new("Power Usage (%)", power_points)
                            .color(Color32::from_rgb(255, 0, 128)),
                    );
                });
            // ==================== 逻辑修改结束 ====================

            ui.add_space(12.0);
            ui.separator();

            ui.heading("🧩 GPU Processes");
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    let processes = self.processes.lock().unwrap();
                    egui::Grid::new("processes_grid")
                        .striped(true)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("PID").strong());
                            ui.label(egui::RichText::new("Name").strong());
                            ui.label(egui::RichText::new("Memory (MB)").strong());
                            ui.end_row();
                            for proc in processes.iter() {
                                ui.label(proc.pid.to_string());
                                ui.label(&proc.name);
                                ui.label(format!(
                                    "{:.1}",
                                    proc.memory_usage as f64 / 1024.0 / 1024.0
                                ));
                                ui.end_row();
                            }
                        });
                });
        });

        ctx.request_repaint();
    }
}

fn main() {
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size([1000.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "RGM",
        native_options,
        Box::new(|cc| Ok(Box::new(RgmApp::new(cc)))),
    )
    .expect("Failed to start application");
}
