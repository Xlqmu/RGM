use crossbeam_channel::{Receiver, bounded};
use eframe::egui;
use egui::ViewportBuilder;
use egui_plot::{GridMark, Line, Plot, PlotPoints};
use nvml_wrapper::Nvml;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::{sync::Arc, thread, time::Duration};

// 存储 GPU 数据的结构
#[derive(Clone, Debug)]
struct GpuData {
    timestamp: f64,    // 时间戳（秒）
    utilization: f32,  // GPU 利用率 (%)
    memory_used: f64,  // 已用显存 (GB)
    memory_total: f64, // 总显存 (GB)
    temperature: u32,  // 温度 (°C)
}

// 应用程序状态
struct RgmApp {
    data: Arc<Mutex<VecDeque<GpuData>>>, // 历史数据
    receiver: Receiver<GpuData>,         // 从后台线程接收数据的通道
    _start_time: std::time::Instant,     // 程序开始时间 (加下划线表示有意未使用)
    max_data_points: usize,              // 保留的最大数据点数
    display_duration: f64,               // 显示的时间范围（秒）
}

impl RgmApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 创建通道用于线程通信
        let (sender, receiver) = bounded(100);

        // 创建一个共享的数据队列
        let data = Arc::new(Mutex::new(VecDeque::with_capacity(300))); // 5分钟，每秒1个点
        let data_clone = Arc::clone(&data);

        // 启动后台线程收集 GPU 数据
        thread::spawn(move || {
            // 初始化 NVML
            let nvml = match Nvml::init() {
                Ok(nvml) => nvml,
                Err(err) => {
                    eprintln!("Failed to initialize NVML: {}", err);
                    return;
                }
            };

            // 获取 GPU 设备
            let device = match nvml.device_by_index(0) {
                Ok(device) => device,
                Err(err) => {
                    eprintln!("Failed to get GPU device: {}", err);
                    return;
                }
            };

            let start_time = std::time::Instant::now();

            loop {
                // 获取 GPU 数据
                match (
                    device.utilization_rates(),
                    device.memory_info(),
                    device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu),
                ) {
                    (Ok(util), Ok(mem), Ok(temp)) => {
                        let gpu_data = GpuData {
                            timestamp: start_time.elapsed().as_secs_f64(),
                            utilization: util.gpu as f32,
                            memory_used: mem.used as f64 / 1024.0 / 1024.0 / 1024.0, // 转换为 GB
                            memory_total: mem.total as f64 / 1024.0 / 1024.0 / 1024.0,
                            temperature: temp,
                        };

                        // 发送数据到主线程
                        if sender.send(gpu_data).is_err() {
                            break; // 通道已关闭，退出线程
                        }
                    }
                    _ => {
                        eprintln!("Failed to get GPU information");
                    }
                }

                // 每秒采集5次数据
                thread::sleep(Duration::from_millis(200));
            }
        });

        // 设置 UI 主题
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.dark_mode = true;
        cc.egui_ctx.set_style(style);

        Self {
            data: data_clone,
            receiver,
            _start_time: std::time::Instant::now(),
            max_data_points: 1500,  // 默认保留5分钟的数据(5min * 60s * 5/1s)
            display_duration: 60.0, // 默认显示1分钟的图表
        }
    }
}

impl eframe::App for RgmApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 从通道接收新数据
        while let Ok(gpu_data) = self.receiver.try_recv() {
            let mut data = self.data.lock().unwrap();
            data.push_back(gpu_data);

            // 如果数据点太多，移除旧的数据
            while data.len() > self.max_data_points {
                data.pop_front();
            }
        }

        // 中央面板
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Rust GPU Monitor");

            let data_guard = self.data.lock().unwrap();

            // 如果有数据，显示最新的 GPU 信息
            if let Some(latest) = data_guard.back() {
                ui.horizontal(|ui| {
                    ui.label(format!("GPU: {}%", latest.utilization));
                    ui.label(format!(
                        "Memory: {:.2}/{:.2} GB",
                        latest.memory_used, latest.memory_total
                    ));
                    ui.label(format!("Temperature: {}°C", latest.temperature));
                });
            }

            // 创建数据点用于绘制曲线
            let gpu_util_points: PlotPoints = data_guard
                .iter()
                .map(|data| [data.timestamp, data.utilization as f64])
                .collect();

            let memory_points: PlotPoints = data_guard
                .iter()
                .map(|data| [data.timestamp, data.memory_used / data.memory_total * 100.0])
                .collect();

            let temp_points: PlotPoints = data_guard
                .iter()
                .map(|data| [data.timestamp, data.temperature as f64])
                .collect();

            // 计算显示的时间范围
            let now = data_guard.back().map_or(0.0, |d| d.timestamp);

            // 实际使用 x_min 和 x_max 来设置图表范围
            let x_min = (now - self.display_duration).max(0.0);
            let x_max = now;

            // 绘制 GPU 利用率曲线
            Plot::new("gpu_utilization")
                .view_aspect(2.0)
                .set_margin_fraction(egui::Vec2::new(0.0, 0.2))
                .include_y(0.0)
                .include_y(100.0)
                .x_axis_formatter(|x, _range| format!("{:?}", x))
                .y_axis_formatter(|y, _range| format!("{:?}%", y))
                .show_x(true)
                .show_y(true)
                .x_grid_spacer(|_| vec![])
                // 添加 x 轴范围设置
                .include_x(x_min)
                .include_x(x_max)
                .y_grid_spacer(|_| {
                    vec![
                        GridMark {
                            value: 10.0,
                            step_size: 10.0,
                        },
                        GridMark {
                            value: 30.0,
                            step_size: 20.0,
                        },
                        GridMark {
                            value: 50.0,
                            step_size: 20.0,
                        },
                        GridMark {
                            value: 70.0,
                            step_size: 20.0,
                        },
                        GridMark {
                            value: 90.0,
                            step_size: 20.0,
                        },
                    ]
                })
                .show(ui, |plot_ui| {
                    // GPU 利用率曲线
                    plot_ui.line(
                        Line::new("GPU Utilization", gpu_util_points)
                            .color(egui::Color32::from_rgb(0, 255, 0)),
                    );

                    // 内存利用率曲线
                    plot_ui.line(
                        Line::new("Memory Usage", memory_points)
                            .color(egui::Color32::from_rgb(0, 128, 255)),
                    );

                    // 温度曲线
                    let scaled_temp_points: PlotPoints = temp_points
                        .points()
                        .iter()
                        .map(|point| [point.x, point.y])
                        .collect();

                    plot_ui.line(
                        Line::new("Temperature (°C)", scaled_temp_points)
                            .color(egui::Color32::from_rgb(255, 128, 0)),
                    );
                });

            // 控制区域
            ui.horizontal(|ui| {
                ui.label("显示时间范围:");
                if ui.button("30秒").clicked() {
                    self.display_duration = 30.0;
                }
                if ui.button("1分钟").clicked() {
                    self.display_duration = 60.0;
                }
                if ui.button("5分钟").clicked() {
                    self.display_duration = 300.0;
                }
                if ui.button("全部").clicked() {
                    self.display_duration = now;
                }
            });
        });

        // 请求持续更新 UI
        ctx.request_repaint();
    }
}

fn main() {
    let native_options = eframe::NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Rust GPU Monitor",
        native_options,
        Box::new(|cc| Ok(Box::new(RgmApp::new(cc)))),
    )
    .expect("Failed to start application");
}
