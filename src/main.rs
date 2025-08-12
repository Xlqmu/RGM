use nvml_wrapper::Nvml;

fn main() {
    let nvml = Nvml::init().unwrap();
    let device = nvml.device_by_index(0).unwrap();
    let util = device.utilization_rates().unwrap();
    println!("GPU Utilization: {}%", util.gpu);
}
