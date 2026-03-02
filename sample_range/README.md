# FlightScope Driving Range

A Vulkan-rendered 3D driving range that visualizes live golf shot data from a FlightScope (or compatible) launch monitor. Shot data flows through [flighthook](https://github.com/divotmaker/flighthook), a middleware bridge that intercepts the device's WiFi protocol and exposes it over a local WebSocket.

```
FlightScope device  →  flighthook (WebSocket server)  →  this app (Vulkan renderer)
```

The driving range displays ball flight, roll-out, a ball trail, HUD panels with full shot metrics, and distance markers at 50/100/150/200 yards. When connected to flighthook, the ball sits at the tee between shots and animates whenever a new shot arrives.

---

## Table of Contents

1. [Hardware Requirements](#hardware-requirements)
2. [System Dependencies](#system-dependencies)
3. [Install Rust](#install-rust)
4. [Build the Driving Range](#build-the-driving-range)
5. [Set Up flighthook](#set-up-flighthook)
6. [Network Setup](#network-setup)
7. [Running Live Mode](#running-live-mode)
8. [Running Demo Mode](#running-demo-mode)
9. [CLI Reference](#cli-reference)
10. [HUD Layout](#hud-layout)
11. [Windows Setup](#windows-setup)
12. [Troubleshooting](#troubleshooting)

---

## Hardware Requirements

- A FlightScope launch monitor (or Mevo/Mevo+ supported by flighthook) with WiFi
- A Linux or Windows PC with a Vulkan-capable GPU (Intel HD 5500+, AMD GCN+, or any NVIDIA)
- The PC must be able to join the launch monitor's WiFi network (or vice versa)

---

## System Dependencies

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

## Install Rust

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

## Build the Driving Range

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

## Set Up flighthook

flighthook is a separate Rust application that connects to the launch monitor, decodes shot data, and exposes it over a WebSocket at `ws://127.0.0.1:3030/api/ws`.

### 1. Prerequisites for flighthook

The WASM UI (web dashboard) requires Trunk and the wasm32 target. If you only want the native desktop UI, these are not strictly required, but flighthook's Makefile uses them for the default build:

```bash
# Install Trunk
cargo install trunk

# Add the WASM target
rustup target add wasm32-unknown-unknown
```

### 2. Clone and Build flighthook

```bash
git clone https://github.com/mmoore0011/flighthook_integrations.git
cd sample_range

# Full build (native binary + WASM web dashboard)
make build

# --- or, if you want the native binary only without Trunk/WASM ---
cargo build --release
```

### 3. Configure flighthook

On first run, flighthook creates a default config file at:

| OS      | Path |
|---------|------|
| Linux   | `~/.config/flighthook/config.toml` |
| Windows | `%APPDATA%\flighthook\config.toml` |
| macOS   | `~/Library/Application Support/flighthook/config.toml` |

A minimal configuration for a Mevo+ launch monitor looks like this:

```toml
[mevo.0]
name    = "Mevo+"
address = "192.168.2.1:5100"    # IP of the launch monitor on its own WiFi network

[webserver.0]
name = "WebServer"
```

For a mock launch monitor (useful for testing without hardware):

```toml
[mock_monitor.0]
name = "Mock"

[webserver.0]
name = "WebServer"
```

Radar-specific settings for the Mevo+ (all optional, shown with defaults):

```toml
[mevo.0]
name           = "Mevo+"
address        = "192.168.2.1:5100"
ball_type      = "range"
tee_height     = "1.5in"
range          = "8ft"
surface_height = "0in"
track_pct      = 80.0
```

To use a custom config file path:

```bash
make run config=/path/to/my-config.toml

# or directly:
./target/release/flighthook --config /path/to/my-config.toml
```

### 4. Run flighthook

```bash
# From the flighthook directory:
make run

# Headless mode (web dashboard only, no native window):
make run headless=true

# Or run the binary directly:
./target/release/flighthook
```

Once running, flighthook opens a WebSocket server at:

```
ws://127.0.0.1:3030/api/ws
```

The web dashboard (if built with `make build`) is accessible at `http://127.0.0.1:3030`.

---

## Network Setup

The launch monitor creates its own WiFi access point. Your PC needs to connect to it:

1. On your PC, connect to the launch monitor's WiFi network (SSID and password are printed on the device or in the FlightScope app).
2. The launch monitor is typically accessible at `192.168.2.1`. Confirm with `ping 192.168.2.1`.
3. Set the correct IP and port in flighthook's `config.toml` under `[mevo.0] address`.

> **Note:** While connected to the launch monitor's WiFi, your PC will not have internet access. Run flighthook and the driving range on that same machine, or use a second network interface for internet.

---

## Running Live Mode

With flighthook already running and your device connected:

```bash
cd flightscope_vulkan
cargo run --release -- --connect ws://127.0.0.1:3030/api/ws
```

**Expected behavior:**

- The ball sits at the tee on startup, waiting for the first shot.
- When you hit a shot, flighthook delivers the data and the ball animates: parabolic flight → roll-out → 2-second pause → returns to tee.
- The HUD updates with the live shot data during and after the animation.
- If flighthook disconnects or loses the device, the background thread retries silently every 2 seconds. The app stays at the tee.

---

## Running Demo Mode

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

## CLI Reference

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

## HUD Layout

```
┌─────────────────────────────────────────────────────────────────┐
│  Club: Lob Wedge        Shot Type: PushDraw     Player: ...     │  ← top bar
├──────────────┬──────────────────────────────────┬───────────────┤
│ DISTANCES    │                                  │ LAUNCH/SPIN   │
│ Carry  108 y │          3D scene                │ LaunchV  16°  │
│ Roll    21 ft│                                  │ LaunchH   5°  │
│ Total  115 y │                                  │ Spin  9509rpm │
│ Height  45 ft│                                  │ SpinAxis  -9° │
│ ...          │                                  │ ...           │
├──────────────┴──────────────────────────────────┴───────────────┤
│  BallSpd  ClubSpd  SmashFactor  AOA  ClubPath  DynLoft  ...     │  ← bottom bar
└─────────────────────────────────────────────────────────────────┘
```

- **Left panel** — distances (carry, roll, total, height, lateral, flight time, etc.)
- **Right panel** — launch angles, spin, club data
- **Bottom bar** — speed and efficiency stats
- **3D scene** — ball animates along parabolic arc with a trailing ribbon; distance rings at 50/100/150/200 yd

---

## Windows Setup

No source code changes are required. The Rust code is fully cross-platform: `ash` loads `vulkan-1.dll` at runtime, and `ash-window` automatically selects the `VK_KHR_win32_surface` extension on Windows. The steps below replace the Linux system dependencies section.

### 1. Install the LunarG Vulkan SDK

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

### 2. Install Visual Studio Build Tools

Rust on Windows requires a C++ linker. Download **Build Tools for Visual Studio** from https://visualstudio.microsoft.com/visual-cpp-build-tools/ and install the **Desktop development with C++** workload. The full Visual Studio IDE is not needed.

### 3. Install Rust

Download and run `rustup-init.exe` from https://rustup.rs. Accept the defaults — it will install the `stable-x86_64-pc-windows-msvc` toolchain and add `cargo` to your `PATH`.

Open a new terminal after installation to pick up the updated `PATH`, then verify:

```powershell
rustc --version
cargo --version
```

### 4. Build the Driving Range

```powershell
git clone https://github.com/mmoore0011/flightscope_attemp_2.git
cd flightscope_attemp_2\flightscope_vulkan
cargo build --release
```

The compiled binary is at:

```
flightscope_vulkan\target\release\flightscope_vulkan.exe
```

### 5. Set Up and Run flighthook on Windows

Clone and build flighthook natively:

```powershell
git clone https://github.com/divotmaker/flighthook.git
cd flighthook
cargo build --release
```

For the full build with the WASM web dashboard, you also need Trunk:

```powershell
cargo install trunk
rustup target add wasm32-unknown-unknown
make build
```

Run flighthook:

```powershell
.\target\release\flighthook.exe
```

On first run it creates its config at `%APPDATA%\flighthook\config.toml`. Edit it to point at your launch monitor (see [Configure flighthook](#3-configure-flighthook) above — the TOML format is identical on all platforms).

Alternatively, if you already have a Linux machine running flighthook, you can cross-compile a Windows binary from Linux using the flighthook Makefile's deploy target:

```bash
# From Linux, targeting a Windows machine named "golfpc"
make deploy host=golfpc dir=Documents
```

### 6. Run the Driving Range on Windows

```powershell
cd flightscope_vulkan
cargo run --release -- --connect ws://127.0.0.1:3030/api/ws
```

Demo mode (no hardware needed):

```powershell
cargo run --release
cargo run --release -- --csv ..\example_data\4_one_hit_session_export.csv
```

### Windows dependency summary

| What | Linux equivalent | Where to get it |
|---|---|---|
| Vulkan loader + `glslangValidator` | `libvulkan-dev` + `glslang-tools` | [LunarG Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows) |
| C++ linker | `build-essential` | [VS Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) |
| Rust | `rustup` | [rustup.rs](https://rustup.rs) |

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

## Repository Structure

```
flightscope_attemp_2/
├── example_data/
│   ├── 1_wifi_flightscope_connect_and_negotiate.pcapng
│   ├── 2_flightscope_app_connect.pcapng
│   ├── 3_session_start_target_alignment_and_arming.pcapng
│   ├── 4_one_hit_9_iron.pcapng
│   ├── 4_one_hit_session_export.csv
│   └── PROTOCOL_ANALYSIS.md
├── flightscope_app/          ← Godot 4.3 prototype (milestone 1)
└── flightscope_vulkan/       ← Vulkan driving range (milestone 2, active)
    ├── src/
    │   ├── main.rs           ← entry point, CLI args
    │   ├── app.rs            ← animation state machine (Idle/Aerial/Roll/Pause)
    │   ├── shot_data.rs      ← ShotData struct, CSV parser, flighthook parser
    │   ├── flighthook.rs     ← WebSocket background thread
    │   ├── scene/            ← 3D geometry (ground, rings, ball, trail)
    │   ├── hud/              ← HUD vertex builder, font atlas
    │   └── vulkan/           ← Vulkan context, pipelines, buffers, textures
    ├── shaders/              ← GLSL source (compiled to SPIR-V at build time)
    ├── assets/font.ttf
    └── Cargo.toml
```
