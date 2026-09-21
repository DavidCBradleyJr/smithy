use std::path::PathBuf;

fn main() {
    // WebKitGTK's DMA-BUF renderer dies with "Error 71 (Protocol error)" on
    // NVIDIA under Wayland (verified on an RTX 3090 Ti, Hyprland). Respect an
    // explicit user setting, otherwise turn it off before GTK starts.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    extend_path();
    smithy_lib::run()
}

/// Launched from the app launcher, we don't get the login shell's PATH, so
/// agent CLIs installed by Omarchy (~/.local/bin stubs, mise shims) would be
/// invisible. Append the usual locations if they're missing.
fn extend_path() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return };
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for extra in [".local/bin", ".local/share/mise/shims", ".bun/bin", ".cargo/bin"] {
        let dir = home.join(extra);
        if dir.is_dir() && !paths.contains(&dir) {
            paths.push(dir);
        }
    }
    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var("PATH", joined);
    }
}
