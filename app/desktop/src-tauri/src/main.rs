#[cfg(any(target_os = "macos", windows))]
use easy_codex_host::health::{
    DashboardSnapshot, HEALTH_SOCKET_NAME, HealthError, HealthSnapshot, bind_dashboard_slot,
    query_dashboard, query_health,
};
#[cfg(any(target_os = "macos", windows))]
use easy_codex_host::paths::AppPaths;
#[cfg(any(target_os = "macos", windows))]
use serde::Serialize;
#[cfg(windows)]
use tauri::Manager;

#[cfg(any(target_os = "macos", windows))]
#[derive(Debug, Serialize)]
#[serde(tag = "connection", rename_all = "snake_case")]
enum HostProbe {
    Healthy { health: HealthSnapshot },
    Offline { reason: &'static str },
    ProtocolError { reason: &'static str },
}

#[cfg(any(target_os = "macos", windows))]
#[derive(Debug, Serialize)]
#[serde(tag = "connection", rename_all = "snake_case")]
enum DashboardProbe {
    Healthy { dashboard: DashboardSnapshot },
    Offline { reason: &'static str },
    ProtocolError { reason: &'static str },
}

#[cfg(any(target_os = "macos", windows))]
fn app_paths() -> Option<AppPaths> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(|local| AppPaths::from_root(std::path::Path::new(&local).join("EasyCodexInput")))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|home| AppPaths::from_home(std::path::Path::new(&home)))
    }
}

#[cfg(any(target_os = "macos", windows))]
fn is_offline(error: &HealthError) -> bool {
    matches!(
        error,
        HealthError::Io(source)
            if matches!(
                source.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::TimedOut
            )
    )
}

#[cfg(any(target_os = "macos", windows))]
#[tauri::command]
fn host_health() -> HostProbe {
    let Some(paths) = app_paths() else {
        return HostProbe::Offline {
            reason: "home_unavailable",
        };
    };
    match query_health(&paths.runtime_directory.join(HEALTH_SOCKET_NAME)) {
        Ok(health) => HostProbe::Healthy { health },
        Err(error) if is_offline(&error) => HostProbe::Offline {
            reason: "host_unreachable",
        },
        Err(_) => HostProbe::ProtocolError {
            reason: "health_invalid",
        },
    }
}

#[cfg(any(target_os = "macos", windows))]
#[tauri::command]
fn host_dashboard() -> DashboardProbe {
    let Some(paths) = app_paths() else {
        return DashboardProbe::Offline {
            reason: "home_unavailable",
        };
    };
    match query_dashboard(&paths.runtime_directory.join(HEALTH_SOCKET_NAME)) {
        Ok(dashboard) => DashboardProbe::Healthy { dashboard },
        Err(error) if is_offline(&error) => DashboardProbe::Offline {
            reason: "host_unreachable",
        },
        Err(error) => {
            eprintln!("dashboard_probe_failed error={error}");
            DashboardProbe::ProtocolError {
                reason: "dashboard_invalid",
            }
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
#[tauri::command]
fn bind_slot(
    slot: u8,
    task_id: String,
    expected_generation: Option<u64>,
) -> Result<DashboardSnapshot, &'static str> {
    let Some(paths) = app_paths() else {
        return Err("home_unavailable");
    };
    bind_dashboard_slot(
        &paths.runtime_directory.join(HEALTH_SOCKET_NAME),
        slot,
        &task_id,
        expected_generation,
    )
    .map_err(|error| match error {
        HealthError::Rejected(_) => "binding_rejected",
        error if is_offline(&error) => "host_unreachable",
        _ => "dashboard_invalid",
    })
}

#[cfg(any(target_os = "macos", windows))]
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(windows)]
            {
                let Some(paths) = app_paths() else { return Ok(()); };
                let endpoint = paths.runtime_directory.join(HEALTH_SOCKET_NAME);
                if matches!(query_health(&endpoint), Err(ref error) if is_offline(error)) {
                    use std::os::windows::process::CommandExt;
                    use std::process::{Command, Stdio};

                    let host = app.path().resource_dir()?.join("easy-codex-host.exe");
                    if !host.is_file() {
                        eprintln!("bundled_host=missing");
                    } else if let Err(error) = Command::new(host)
                        .arg("daemon")
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .creation_flags(0x0800_0000 | 0x0000_0200)
                        .spawn()
                    {
                        eprintln!("bundled_host=start_failed error={error}");
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host_health,
            host_dashboard,
            bind_slot
        ])
        .run(tauri::generate_context!())
        .expect("Codex Keyboard desktop runtime failed");
}

#[cfg(not(any(target_os = "macos", windows)))]
fn main() {
    println!("Codex Keyboard desktop requires macOS");
}
