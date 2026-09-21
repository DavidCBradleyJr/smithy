//! GPU memory from `nvidia-smi`. The desktop runs on the same GPU, so we
//! budget against memory actually free right now, not the card's total.

use serde::Serialize;

/// Headroom kept free after a load, so the desktop doesn't tip over when
/// a conversation grows its compute buffers.
pub const HEADROOM_MIB: u64 = 512;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Vram {
    pub name: String,
    pub used_mib: u64,
    pub total_mib: u64,
}

impl Vram {
    pub fn free_mib(&self) -> u64 {
        self.total_mib.saturating_sub(self.used_mib)
    }

    pub fn fits(&self, need_mib: u64) -> bool {
        need_mib + HEADROOM_MIB <= self.free_mib()
    }
}

/// Parses `--query-gpu=name,memory.used,memory.total --format=csv,noheader,nounits`.
/// Only the first GPU is used.
pub fn parse_nvidia_smi(out: &str) -> Option<Vram> {
    let line = out.lines().find(|l| !l.trim().is_empty())?;
    let mut parts = line.split(',').map(str::trim);
    let name = parts.next()?.to_string();
    let used_mib = parts.next()?.parse().ok()?;
    let total_mib = parts.next()?.parse().ok()?;
    Some(Vram { name, used_mib, total_mib })
}

pub fn query() -> Option<Vram> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_nvidia_smi(&String::from_utf8_lossy(&out.stdout))
}

#[tauri::command]
pub async fn get_vram() -> Option<Vram> {
    tauri::async_runtime::spawn_blocking(query).await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_first_gpu() {
        let v = parse_nvidia_smi("NVIDIA GeForce RTX 3090 Ti, 20412, 24564\n").unwrap();
        assert_eq!(v.used_mib, 20412);
        assert_eq!(v.total_mib, 24564);
        assert_eq!(v.free_mib(), 4152);
        assert!(v.fits(3000));
        assert!(!v.fits(4000));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_nvidia_smi("").is_none());
        assert!(parse_nvidia_smi("No devices were found").is_none());
    }
}
