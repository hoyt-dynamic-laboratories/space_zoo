# space_zoo v0.45.3

An educational button masher for toddlers, process monitor for grown ups, lots of easter eggs in between. Lightweight, for Linux, designed first for the bare framebuffer console. Animals roam a day/night sky with a gilded sun, phased moon, constellations, and a live system telemetry billboard. Also runs in an X11 window or as an ASCII zoo in a terminal or SSH session.

## Build

Requires Rust and Cargo, a C compiler/linker, and X11 development libraries.
On Debian/Ubuntu, install the native dependencies with:

```sh
sudo apt install build-essential pkg-config libx11-dev libxcursor-dev libxrandr-dev
```

From the project directory:

```sh
cargo build --release --locked
./target/release/space_zoo --version
```

Artwork and animation data are embedded in the executable. The X11 libraries
are build dependencies even for console use; binaries still require compatible system libraries and architecture.

## Run

```sh
./target/release/space_zoo --console  # Bare Linux console (primary mode)
./target/release/space_zoo --window   # X11 window
./target/release/space_zoo --ascii    # Terminal or SSH
./target/release/space_zoo            # Auto-select mode
```

Console mode requires access to a Linux framebuffer (usually `/dev/fb0`). Set
`FRAMEBUFFER=/dev/fb1` to use another device. Exit with **Ctrl+C** before
switching virtual consoles.

Window mode uses X11; Wayland requires XWayland and a working `DISPLAY`.
ASCII mode requires at least 40 columns and 14 rows. Auto-select prefers a
physical console, then ASCII for SSH or no `DISPLAY`, otherwise X11.

## Controls

| Key | Action |
| --- | --- |
| Arrow keys | Steer the dog |
| Enter | Call the heron |
| C | Release hens (daytime) |
| Escape | Call the helicopter |
| 1 | Extra Chupacabra visit (night) |
| 2 | Extra Sasquatch visit (night) |
| Space / other keys | Animal reactions |
| Ctrl+C | Exit |

Each mythical visitor allows one extra requested visit per night.

## Options

| Flag | Purpose |
| --- | --- |
| `--help` | Show help and controls |
| `--version`, `-V` | Show embedded version |
| `--hour 0..23` | Set scene hour while animations continue |
| `--window --animal` | Sprite viewer (arrows, C, F, Space, Escape) |
| `--constellation` | Accelerate celestial motion for inspection |
| `--debug` | Pixel-world inspection controls |
| `--bench-frames N` | Headless night rendering benchmark |
| `--bench-sun N` | Headless day rendering benchmark |

Benchmark flags must be the first argument; `N` is 1–100000. They measure
rendering performance, not overall process CPU use.

## Development

```sh
cargo test --locked
```

`src/` holds the application, `assets/` the embedded artwork/data inputs, and
`tools/` optional asset generators and checks. Generators may overwrite assets
or generated source; they are not needed for a standard build.

Stable source baseline: **revision 0.453 (surgical compatibility)**.

## License

Copyright (C) 2026 HDL

Licensed under the GNU General Public License, version 2 or later
(`GPL-2.0-or-later`). See [LICENSE](LICENSE).

Third-party dependencies retain their own licenses.