# Third-party software used by Zavyx VE

## Bundled program

The Zavyx VE installers and portable ZIPs include this separate program. Zavyx VE
runs it as an external tool; it keeps its own licence.

### FFmpeg
- Used for: reading videos, audio and images, and encoding exports
- Build: "essentials" build by Gyan Doshi — https://www.gyan.dev/ffmpeg/builds/
- Licence: GNU GPL v3 — https://www.gnu.org/licenses/gpl-3.0.html
- Source code: https://ffmpeg.org/download.html and https://github.com/GyanD/codexffmpeg

## Built in

- **Phosphor Icons** (icon font, via the `egui-phosphor` crate) — MIT licence — https://phosphoricons.com
- Titles use fonts already installed with Windows (Segoe UI, Arial, …); no fonts are shipped.

Zavyx VE itself is freeware under the Zavyx Freeware License (see `LICENSE`). Rust crate
dependencies are listed in `Cargo.lock` with their own licences.
