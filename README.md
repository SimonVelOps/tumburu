Here is a complete and structured README for your project:

## Tumburu DAW

Welcome to the repository for **Tumburu DAW**, a high-performance Digital Audio Workstation written entirely in Rust. Named after the revered Hindu Gandharva (celestial musician) divinity, this open-source project is designed for highly efficient audio processing and creation. Audio engineering requires strict real-time guarantees, and Rust's memory safety without a garbage collector makes it the perfect systems language for this task.

### Getting Started

To ensure the best experience, we highly recommend building and running the project in release mode. You can interact with the project using the standard Cargo toolchain:

* `cargo build --release` — Use the release profile to eliminate as much of the real-time processing performance bottleneck as possible.
* `cargo run` — Compiles and executes the DAW locally.
* `cargo test` — Runs the project's test suite to ensure stability across modules.
* `cargo doc` — Generates the documentation locally for offline viewing.

### CI/CD & Documentation

We utilize GitHub Actions to automate our testing and deployment pipelines to maintain high code quality. Our CI/CD setup includes two primary workflows:

* **Rust Workflow:** Automatically executes `build`, `test`, and `doc` verification on pushes and pull requests.
* **Doc Workflow:** Builds the documentation and automatically deploys it directly to GitHub Pages.

You can always view the latest automated, hosted documentation at:
[http://simonvelops.github.io/tumburu_daw/](http://simonvelops.github.io/tumburu_daw/)

### License

This project is proudly open-source and distributed under the GNU General Public License v2.0 (GPLv2). Contributions, feature requests, and bug reports are highly encouraged to help Tumburu reach its full celestial potential.
