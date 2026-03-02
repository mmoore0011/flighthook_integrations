# FlightScope Driving Range

A Vulkan-rendered 3D driving range that visualizes live golf shot data from a FlightScope (or compatible) launch monitor. Shot data flows through [flighthook](https://github.com/divotmaker/flighthook), a middleware bridge that intercepts the device's WiFi protocol and exposes it over a local WebSocket.

```
FlightScope device  →  flighthook (WebSocket server)  →  this app (Vulkan renderer)
```

The driving range displays ball flight, roll-out, a ball trail, HUD panels with full shot metrics, and distance markers at 50/100/150/200 yards. When connected to flighthook, the ball sits at the tee between shots and animates whenever a new shot arrives.

---

## Run
### Set Up flighthook

flighthook is a separate Rust application that connects to the launch monitor, decodes shot data, and exposes it over a WebSocket at `ws://127.0.0.1:3030/api/ws`.

#### 1. Download and Run Flighthook
https://github.com/divotmaker/flighthook/releases

```
flighthook-windows-x86_64.exe
```

#### 2. (Optional) Remove GSPro connection from Flighthook
On first run it creates its config at `%APPDATA%\flighthook\config.toml`. Edit it to remove the GSPro connection if you don't have GSPro.

```
vi $APPDATA/flighthook/config.toml

...
[REMOVE THIS SECTION]
[gspro.0]
name = "Local GSPro"
address = "127.0.0.1:921"
```

#### 3. Kill and restart Flighthook

Verify that it connects to the flightscope and that the Webserver is active.  If you have issues with the flightscope connection see "Network Setup"

Once running, flighthook opens a WebSocket server at:

```
ws://127.0.0.1:3030/api/ws
```

The web dashboard is accessible at `http://127.0.0.1:3030`.

---

### Network Setup

The launch monitor creates its own WiFi access point. Your PC needs to connect to it:

1. On your PC, connect to the launch monitor's WiFi network (SSID and password are printed on the device or in the FlightScope app).
2. The launch monitor is typically accessible at `192.168.2.1`. Confirm with `ping 192.168.2.1`.
3. Set the correct IP and port in flighthook's `config.toml` under `[mevo.0] address`.

> **Note:** While connected to the launch monitor's WiFi, your PC will not have internet access. Run flighthook and the driving range on that same machine, or use a second network interface for internet.

---

### Running Live Mode

With flighthook already running and your device connected:

```bash
cd target
cargo run --release -- --connect ws://127.0.0.1:3030/api/ws
```

**Expected behavior:**

- The ball sits at the tee on startup, waiting for the first shot.
- When you hit a shot, flighthook delivers the data and the ball animates: parabolic flight → roll-out → 2-second pause → returns to tee.
- The HUD updates with the live shot data during and after the animation.
- If flighthook disconnects or loses the device, the background thread retries silently every 2 seconds. The app stays at the tee.

---

### Running Demo Mode

No hardware or flighthook required. Uses a bundled example shot (Lob Wedge, 115 yards):

```bash
cd flightscope_vulkan
cargo run --release
```

Load your own CSV export from a FlightScope session:

```bash
cargo run --release -- --csv ../example_data/4_one_hit_session_export.csv
```

In demo mode, the animation loops continuously.

---

### CLI Reference

```
flightscope_vulkan [OPTIONS]

Options:
  --connect <url>            Connect to flighthook WebSocket (live mode)
                             Example: --connect ws://127.0.0.1:3030/api/ws

  --csv <path>               Load shot data from a FlightScope CSV export
                             Example: --csv my_session.csv

  --screenshot <path>        Render one frame, save as PNG, and exit
                             Example: --screenshot out.png

  --screenshot-frame <n>     Render N frames before capturing (default: 1)
                             Example: --screenshot-frame 5
```

**Mode priority:** `--connect` takes precedence over `--csv`. If neither is given, the built-in example shot is used.

---
## Build
### Hardware Requirements

- A FlightScope launch monitor (or Mevo/Mevo+ supported by flighthook) with WiFi
- A Linux or Windows PC with a Vulkan-capable GPU (Intel HD 5500+, AMD GCN+, or any NVIDIA)
- The PC must be able to join the launch monitor's WiFi network (or vice versa)

---

### Linux Build
#### System Dependencies

Install the required system packages (Ubuntu 22.04 / Debian):

```bash
sudo apt update
sudo apt install \
    libvulkan-dev \
    vulkan-validationlayers-dev \
    glslang-tools \
    spirv-tools \
    pkg-config \
    libssl-dev \
    build-essential \
    git
```

Verify Vulkan is functional:

```bash
vulkaninfo --summary
```

If `vulkaninfo` is not installed: `sudo apt install vulkan-tools`.

---

#### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup update stable
```

To persist `cargo` in your PATH across sessions, add this to `~/.bashrc`:

```bash
source "$HOME/.cargo/env"
```

---

#### Build the Driving Range

Clone this repository and build the Vulkan app:

```bash
git clone https://github.com/mmoore0011/flighthook_integrations.git
cd sample_range 
cargo build --release
```

The build script (`build.rs`) compiles GLSL shaders to SPIR-V automatically using `glslangValidator`. No manual shader compilation step is needed.

The compiled binary is at:

```
flightscope_vulkan/target/release/flightscope_vulkan
```

---

### Windows Build

No source code changes are required. The Rust code is fully cross-platform: `ash` loads `vulkan-1.dll` at runtime, and `ash-window` automatically selects the `VK_KHR_win32_surface` extension on Windows. The steps below replace the Linux system dependencies section.

#### 1. Install the LunarG Vulkan SDK

Download and run the installer from https://vulkan.lunarg.com/sdk/home#windows.

The SDK installs:
- `vulkan-1.dll` — the Vulkan loader that `ash` finds at runtime
- `glslangValidator.exe` — used by `build.rs` to compile shaders at build time
- Validation layers and `vulkaninfo`

After installation, verify Vulkan is working by opening a new Command Prompt or PowerShell and running:

```powershell
vulkaninfo --summary
```

The SDK installer adds its `Bin` directory to your `PATH` automatically. If `vulkaninfo` is not found, add `C:\VulkanSDK\<version>\Bin` to your `PATH` manually.

#### 2. Install Visual Studio Build Tools

Rust on Windows requires a C++ linker. Download **Build Tools for Visual Studio** from https://visualstudio.microsoft.com/visual-cpp-build-tools/ and install the **Desktop development with C++** workload. The full Visual Studio IDE is not needed.

#### 3. Install Rust

Download and run `rustup-init.exe` from https://rustup.rs. Accept the defaults — it will install the `stable-x86_64-pc-windows-msvc` toolchain and add `cargo` to your `PATH`.

Open a new terminal after installation to pick up the updated `PATH`, then verify:

```powershell
rustc --version
cargo --version
```

#### 4. Build the Driving Range

```powershell
git clone https://github.com/mmoore0011/flighthook_integrations.git
cd sample_range
cargo build --release
```

The compiled binary is at:

```
flightscope_vulkan\target\release\flightscope_vulkan.exe
```

---

## Troubleshooting

**`glslangValidator: command not found` during build**
```bash
sudo apt install glslang-tools
```

**`vulkan: no physical devices found` or blank window**
- Check that your GPU driver supports Vulkan: `vulkaninfo --summary`
- On Intel integrated graphics, ensure Mesa is up to date: `sudo apt install mesa-vulkan-drivers`

**flighthook won't connect to the Mevo+**
- Confirm your PC is on the launch monitor's WiFi and can reach `192.168.2.1`
- Verify the `address` field in `config.toml` matches the device IP and port
- Try the mock monitor config first to confirm the WebSocket pipeline works end-to-end

**App stays at tee indefinitely in live mode**
- Confirm flighthook is running: open `http://127.0.0.1:3030` in a browser
- Check flighthook's terminal output for device connection status
- The driving range logs connection attempts and errors to stderr — run with `RUST_LOG=debug` if needed
- Verify you passed the correct WebSocket URL: `--connect ws://127.0.0.1:3030/api/ws`

**`cargo: command not found`**
```bash
source "$HOME/.cargo/env"
```

**CSV file not loading**
- The CSV must be a FlightScope session export (not a manual spreadsheet)
- The parser expects the FlightScope column layout; check `example_data/4_one_hit_session_export.csv` as a reference

---
