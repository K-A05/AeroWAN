mod daemon; // declares : daemon, config, signals
mod network;
mod transport; // contains transport implementations for reticulum and iroh, to create and manage transport endpoints.
mod tui;
mod types;
mod utils; // implements indentity manangement for Iroh and Reticulum

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     let args: Vec<String> = std::env::args().collect();
//     let run_tui = args.get(1).map(|a| a == "--tui").unwrap_or(false);

//     let daemon = daemon::Daemon::new().await?;
//     daemon.run().await?;
//     if run_tui {
//         // Spawn TUI as a child, wait for it to exit, then shut down
//         let exe = std::env::current_exe()?;
//         let mut tui_child = tokio::process::Command::new(exe)
//             .arg("--tui-only") // internal flag, just runs the TUI with no daemon logic
//             .spawn()?;

//         tui_child.wait().await?;
//         // TUI exited — fall through to daemon shutdown
//     } else {
//         daemon::signals::wait_for_shutdown().await?;
//     }

//     Ok(())
// }
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let run_tui = args.get(1).map(|a| a == "--tui").unwrap_or(false);
    let tui_only = args.get(1).map(|a| a == "--tui-only").unwrap_or(false);

    if tui_only {
        tui::run().await?;
        return Ok(());
    }

    let daemon = daemon::Daemon::new().await?;

    if run_tui {
        let exe = std::env::current_exe()?;
        let mut tui_child = tokio::process::Command::new(exe)
            .arg("--tui-only")
            .spawn()?;

        tui_child.wait().await?;
        // TUI exited — fall through to daemon shutdown
    } else {
        daemon::signals::wait_for_shutdown().await?;
    }

    daemon.run().await?;
    Ok(())
}
