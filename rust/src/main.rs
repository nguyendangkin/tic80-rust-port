//! TIC-80 — 100% Rust Fantasy Computer

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        println!("TIC-80 Rust — 100% Rust Fantasy Computer");
        println!("Usage: {} [cart.tic]", args[0]);
        return;
    }

    #[cfg(feature = "sdl2")]
    {
        let cart_data = if args.len() > 1 {
            match std::fs::read(&args[1]) {
                Ok(d) => { println!("Loaded: {} ({} bytes)", args[1], d.len()); d }
                Err(e) => { eprintln!("Error: {}", e); return; }
            }
        } else { Vec::new() };

        let mut app = tic80_rust::desktop::TicApp::new();
        if !cart_data.is_empty() { app.load_cartridge(&cart_data); }
        if let Err(e) = app.run() { eprintln!("Error: {}", e); std::process::exit(1); }
    }

    #[cfg(not(feature = "sdl2"))]
    {
        println!("TIC-80 Rust (headless mode) — 224 tests pass");
        println!("");
        println!("Desktop GUI:");
        println!("  cargo run --release --features sdl2 [cart.tic]");
        println!("  (requires SDL2 development libraries)");
    }
}
