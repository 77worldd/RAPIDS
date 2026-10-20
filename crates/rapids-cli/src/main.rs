mod cli;
mod render;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use tokio::sync::Notify;

use rapids_engine::{
    spawn_download, DownloadSpec, DownloadState, EngineConfig, EngineEvent, Workspace,
};

#[tokio::main]
async fn main() -> ExitCode {
    match cli::Cli::parse().command {
        cli::Command::Get(args) => run_get(args).await,
        cli::Command::Cancel(args) => run_cancel(args).await,
    }
}

async fn run_get(args: cli::GetArgs) -> ExitCode {
    let spec = DownloadSpec {
        url: args.url,
        dest: args.output,
        expected_sha256: args.sha256,
        connections: args.connections,
        overwrite: args.overwrite,
    };
    let (handle, mut events) = match spawn_download(spec, EngineConfig::default()) {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("rapids: {error}");
            return ExitCode::from(3);
        }
    };

    // The first interrupt pauses (the engine checkpoints on the way down)
    // and this process then exits with the interrupted code. A second
    // interrupt is picked up by the signal task, which notifies the main
    // loop to leave right away.
    let exit_notify = Arc::new(Notify::new());
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            handle.pause().await;
        }
    });
    let signal_exit = Arc::clone(&exit_notify);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tokio::signal::ctrl_c().await;
        signal_exit.notify_one();
    });

    let mut writer = render::LineWriter::new();
    loop {
        tokio::select! {
            _ = exit_notify.notified() => {
                writer.finish_line();
                return ExitCode::from(130);
            }
            incoming = events.recv() => {
                let Some(event) = incoming else {
                    writer.finish_line();
                    eprintln!("rapids: engine stopped without a final event");
                    return ExitCode::from(3);
                };
                writer.write(&event);
                match event {
                    EngineEvent::StateChanged {
                        state: DownloadState::Paused,
                    } => {
                        return ExitCode::from(2);
                    }
                    EngineEvent::Completed { .. } => return ExitCode::SUCCESS,
                    EngineEvent::Failed { .. } | EngineEvent::Cancelled => {
                        return ExitCode::from(3);
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn run_cancel(args: cli::CancelArgs) -> ExitCode {
    let dest = destination_of(&args.target);
    let mut workspace = Workspace::for_dest(&dest);
    if let Err(error) = workspace.create_and_lock().await {
        eprintln!("rapids: {error}");
        return ExitCode::from(3);
    }
    if !workspace.has_history().await {
        let _ = workspace.discard().await;
        eprintln!("rapids: no RAPIDS workspace found for {}", dest.display());
        return ExitCode::from(3);
    }
    match workspace.discard().await {
        Ok(()) => {
            println!("removed workspace for {}", dest.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("rapids: {error}");
            ExitCode::from(3)
        }
    }
}

/// Accepts either the destination path or the sidecar directory itself.
fn destination_of(target: &Path) -> PathBuf {
    let sidecar_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".rapids"))
        .map(str::to_string);
    match sidecar_name {
        Some(stripped) => match target.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(stripped),
            _ => PathBuf::from(stripped),
        },
        None => target.to_path_buf(),
    }
}
