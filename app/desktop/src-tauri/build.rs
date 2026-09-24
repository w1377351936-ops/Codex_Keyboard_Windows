#[cfg(any(target_os = "macos", windows))]
fn main() {
    tauri_build::build()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn main() {}
