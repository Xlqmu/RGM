# RGM: Rust GPU Monitor

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)](https://github.com/xlqmu/RGM)
[![License: MIT / Apache-2.0](https://img.shields.io/badge/License-MIT%20%2F%20Apache--2.0-blue)](https://opensource.org/licenses/MIT)

A lightweight, command-line utility built with Rust to quickly check your NVIDIA GPU's utilization. Simple, fast, and reliable.

## Features

*   **Instant Readout:** Get the current GPU utilization 
percentage immediately.
*   **Minimalist:** No complex UI, just the data you need.
*   **Low Overhead:** Built in Rust for maximum performance and minimal resource consumption.

## Prerequisites

Before you begin, ensure you have the following installed on your system:

1.  **Rust & Cargo:** If you don't have them, install them from [rust-lang.org](https://www.rust-lang.org/).
2.  **NVIDIA Drivers:** You must have the official NVIDIA drivers installed. You can verify this by running `nvidia-smi` in your terminal.

## Installation

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/your-username/RGM.git
    cd RGM
    ```

2.  **Build the optimized binary:**
    ```bash
    cargo build --release
    ```

The final executable will be located at `target/release/RGM`.

## Usage

Run the compiled application from your terminal to see the current GPU status.

```bash
./target/release/RGM
```

**Example Output:**
```
GPU Utilization: 18%
```

---

## Troubleshooting

#### Error: `libnvidia-ml.so: cannot open shared object file: No such file or directory`

This is a common runtime issue on Linux systems. It occurs when the application cannot find the NVIDIA Management Library (NVML), even if `nvidia-smi` works correctly. It's typically caused by a missing symbolic link in the system's library paths.

**Solution:**

1.  **Find the NVML library path.** Use `ldconfig` to locate the actual library file.
    ```bash
    ldconfig -p | grep libnvidia-ml.so.1
    ```
    Note the path in the output, which will look something like `=> /lib/x86_64-linux-gnu/libnvidia-ml.so.1`.

2.  **Create a symbolic link.** Use the path from the previous step to create the link that the application expects.
    ```bash
    # IMPORTANT: Use the path you found on your system.
    sudo ln -s /lib/x86_64-linux-gnu/libnvidia-ml.so.1 /lib/x86_64-linux-gnu/libnvidia-ml.so
    ```

After creating the link, the application should run without issues.

## License

This project is licensed under either of:

*   Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
*   MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dually licensed as above, without any additional terms or conditions.