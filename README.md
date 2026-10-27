# Better Generic DAW

[![CI](https://img.shields.io/github/actions/workflow/status/utopic-future/Better-Generic-DAW/rust.yml?style=for-the-badge&label=CI)](https://github.com/utopic-future/Better-Generic-DAW/actions/workflows/rust.yml)
[![License: GPL-3.0-only](https://img.shields.io/badge/License-GPL--3.0-blue.svg?style=for-the-badge)](LICENSE)

**Better Generic DAW** is an independent fork of [Generic DAW](https://github.com/generic-daw/generic-daw), an open-source digital audio workstation written in Rust. Development here focuses on UI performance, usability, and workflow improvements.

This project is in active development. Features and behavior may change.

## Features

- Arrange and edit audio and MIDI clips.
- Record audio and MIDI; edit MIDI in the piano roll.
- Mix and route tracks and channels.
- Host CLAP, VST2, and VST3 plugins.
- Use configurable keyboard shortcuts.
- Play notes with the computer keyboard: select a track, then press **Caps Lock** to toggle MIDI keyboard mode. The Z/S/X/D/C/V/G/B/H/N/J/M and Q/2/W/3/E/R/5/T/6/Y/7/U keys play two chromatic octaves.

## Build from source

### Requirements

- Rust and Cargo from the [stable Rust toolchain](https://rustup.rs).
- On Linux, install the ALSA and PipeWire development packages:
  - Debian or Ubuntu: `sudo apt install libasound2-dev libpipewire-0.3-dev`
  - Fedora: `sudo dnf install alsa-lib-devel pipewire-devel`
  - Arch Linux: `sudo pacman -S alsa-lib libpipewire`
- Download the Lucide icon font into the repository root before building. The build script subsets it for the app.

### Clone and build

```sh
git clone https://github.com/utopic-future/Better-Generic-DAW.git
cd Better-Generic-DAW
curl -L https://unpkg.com/lucide-static@latest/font/Lucide.ttf -o Lucide.ttf
cargo build --release
```

In Windows PowerShell, use this command to download the font:

```powershell
Invoke-WebRequest -Uri https://unpkg.com/lucide-static@latest/font/Lucide.ttf -OutFile Lucide.ttf
```

The executable is written to `target/release/generic_daw` (`target/release/generic_daw.exe` on Windows).

## Builds

GitHub Actions checks the workspace on Ubuntu, Windows, and macOS. Runs on the `main` branch also build platform binaries and upload them as workflow artifacts; this fork does not currently publish versioned releases. See [Actions](https://github.com/utopic-future/Better-Generic-DAW/actions).

## Contributing

Please open issues and pull requests in this repository. For the upstream project, see [Generic DAW](https://github.com/generic-daw/generic-daw).

## License

This fork is distributed under GPL-3.0-only. See [LICENSE](LICENSE).
