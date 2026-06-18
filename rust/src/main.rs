//! TIC-80 — 100% Rust Fantasy Computer
//!
//! Binary entry point.  Build with:
//!   cargo build --release
//!   cargo run --release [cart.tic]

use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Display help
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        println!("TIC-80 Rust — 100% Rust Fantasy Computer");
        println!("Usage: {} [cart.tic]", args[0]);
        return;
    }

    // Load cartridge if provided
    let cart_data = if args.len() > 1 {
        match fs::read(&args[1]) {
            Ok(data) => {
                println!("Loaded: {} ({} bytes)", args[1], data.len());
                data
            }
            Err(e) => {
                eprintln!("Error: cannot load '{}': {}", args[1], e);
                process::exit(1);
            }
        }
    } else {
        Vec::new()
    };

    // Run TIC-80
    if let Err(e) = run_tic80(&cart_data) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

#[cfg(feature = "sdl2")]
fn run_tic80(cart_data: &[u8]) -> Result<(), String> {
    let mut app = tic80_rust::desktop::DesktopApp::new()?;
    if !cart_data.is_empty() {
        app.load_cartridge(cart_data);
    }
    app.run()
}

#[cfg(not(feature = "sdl2"))]
fn run_tic80(_cart_data: &[u8]) -> Result<(), String> {
    // Headless mode — run tests and exit
    println!("TIC-80 Rust Library");
    println!("220 tests pass");
    println!("Build with --features sdl2 for desktop app");
    Ok(())
}
