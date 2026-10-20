//! Entry point: logging, panic hook, CLI (`gallerybrowser [PATH...]`), single instance
//! (unix socket at AppDirs::instance_socket: a second launch forwards its paths and exits 0),
//! eframe window (title "GalleryBrowser", min 1200x800, app_id io.github.nickpittas.GalleryBrowser).

mod args;
mod crash;
mod ipc;

use std::process::ExitCode;
use std::sync::{OnceLock, mpsc};

use gallerybrowser_core::AppDirs;
use gallerybrowser_ui_egui::{GalleryApp, StartupOptions};

const APP_ID: &str = "io.github.nickpittas.GalleryBrowser";

/// The egui context, published by the eframe creator so the IPC thread can wake the UI.
static EGUI_CTX: OnceLock<egui::Context> = OnceLock::new();

fn main() -> ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();
    crash::install();

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
    let paths = match args::parse(std::env::args_os().skip(1), &cwd) {
        Ok(args::Cli::Help) => {
            print!("{}", args::HELP);
            return ExitCode::SUCCESS;
        }
        Ok(args::Cli::Version) => {
            println!("gallerybrowser {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(args::Cli::Run(paths)) => paths,
        Err(msg) => {
            eprintln!("gallerybrowser: {msg}");
            return ExitCode::FAILURE;
        }
    };

    let dirs = match AppDirs::resolve() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("gallerybrowser: cannot set up its directories: {e}");
            return ExitCode::FAILURE;
        }
    };
    crash::set_log_dir(&dirs.cache.join("crash"));

    // Single instance: forward to a running one, or become the one that listens.
    let socket = dirs.instance_socket();
    let mut _socket_guard = None;
    let mut incoming = None;
    match ipc::claim(&socket, &paths) {
        ipc::Claim::Forwarded => {
            log::info!("handed {} path(s) to the running instance", paths.len());
            return ExitCode::SUCCESS;
        }
        ipc::Claim::Primary(listener) => {
            _socket_guard = Some(ipc::SocketGuard(socket));
            let (tx, rx) = mpsc::channel();
            incoming = Some(rx);
            let spawned = std::thread::Builder::new()
                .name("gb-instance-ipc".into())
                .spawn(move || {
                    ipc::serve(listener, move |paths| {
                        if !paths.is_empty() && tx.send(paths).is_err() {
                            return;
                        }
                        if let Some(ctx) = EGUI_CTX.get() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                            ctx.request_repaint();
                        }
                    });
                });
            if let Err(e) = spawned {
                log::warn!("could not start the instance listener thread: {e}");
            }
        }
        ipc::Claim::Unavailable(e) => {
            log::warn!(
                "single-instance socket unavailable ({}): {e}",
                socket.display()
            );
        }
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GalleryBrowser")
            .with_app_id(APP_ID)
            .with_min_inner_size([1200.0, 800.0])
            .with_inner_size([1600.0, 1000.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let opts = StartupOptions { paths, incoming };

    let result = eframe::run_native(
        "GalleryBrowser",
        native_options,
        Box::new(move |cc| {
            let _ = EGUI_CTX.set(cc.egui_ctx.clone());
            let app = GalleryApp::new(cc, opts)?;
            // Paths forwarded while the app was starting are already queued; make sure they are seen.
            cc.egui_ctx.request_repaint();
            Ok(Box::new(app))
        }),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let msg = format!("GalleryBrowser failed: {e}");
            log::error!("{msg}");
            crash::record(&msg);
            eprintln!("gallerybrowser: {e}");
            ExitCode::FAILURE
        }
    }
}
