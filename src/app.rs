use crate::data::{GpuData, GpuInfo, ProcessInfo};
use crate::monitor::create_monitor;
use crossbeam_channel::{Receiver, bounded};
use eframe::egui::{self, Color32};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::{thread, time::Duration};

// 应用程序状态
pub struct RgmApp {
    data: Arc<Mutex<VecDeque<GpuData>>>,
    receiver: Receiver<(GpuData, Vec<ProcessInfo>)>,
    display_duration: f64,
    gpu_info: GpuInfo,
    processes: Arc<Mutex<Vec<ProcessInfo>>>,
}

impl RgmApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (sender, receiver) = bounded(100);
        let data = Arc::new(Mutex::new(VecDeque::with_capacity(120)));
        let processes = Arc::new(Mutex::new(Vec::new()));

        let monitor = create_monitor().expect("Failed to find and initialize a GPU monitor!");
        let gpu_info = monitor.get_static_info();

        thread::spawn(move || {
            loop {
                match monitor.sample() {
                    Ok((gpu_data, proc_infos)) => {
                        if sender.send((gpu_data, proc_infos)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("Error sampling GPU data: {}", e);
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
        });

        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.dark_mode = true;
        cc.egui_ctx.set_style(style);

        Self {
            data,
            receiver,
            display_duration: 10.0,
            gpu_info,
            processes,
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
            ui.label(format!(
                "{} - Driver: {}",
                self.gpu_info.name, self.gpu_info.driver_version
            ));
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
                            ui.label(format!(
                                "PCIe: Gen {} x{}",
                                self.gpu_info.pcie_gen, self.gpu_info.pcie_width
                            ));
                            ui.label(format!("PCIe TX: {:.2} MB/s", latest.pcie_throughput_tx));
                            ui.label(format!("PCIe RX: {:.2} MB/s", latest.pcie_throughput_rx));
                        });
                    });
                });
            }

            ui.add_space(12.0);
            ui.separator();
            ui.heading("📈 Real-time GPU Metrics (Last 10 Seconds)");

            let latest_timestamp = data_guard.back().map_or(0.0, |d| d.timestamp);
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
                .x_axis_label("Seconds Ago (0 = now)")
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
