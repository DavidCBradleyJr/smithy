//! Starting and stopping model servers.
//!
//! Servers run in their own process group and outlive Smithy, like the
//! runbook's `setsid nohup` launch. Stopping works the same whether Smithy or
//! something else started the server: find who listens on the port, SIGTERM it.

use super::profiles::Profile;
use std::{
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn log_path(id: &str) -> Option<PathBuf> {
    dirs::state_dir().map(|d| d.join("smithy/logs").join(format!("{id}.log")))
}

pub fn spawn(p: &Profile) -> Result<u32, String> {
    if p.start.is_empty() {
        return Err(format!("{} has no start command", p.name));
    }
    spawn_argv(&p.id, &p.start)
}

pub fn spawn_argv(id: &str, argv: &[String]) -> Result<u32, String> {
    let (bin, args) = argv.split_first().ok_or("empty command")?;
    let log = log_path(id).ok_or("no state dir")?;
    fs::create_dir_all(log.parent().unwrap()).map_err(|e| e.to_string())?;
    let out = fs::File::create(&log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0);
    // Scripts like serve.sh expect to run from their own directory.
    if let Some(dir) = Path::new(bin).parent().filter(|d| d.is_dir()) {
        cmd.current_dir(dir);
    }
    let child = cmd.spawn().map_err(|e| format!("{bin}: {e}"))?;
    let pid = child.id();
    // Reap it when it exits so it doesn't linger as a zombie.
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(pid)
}

/// Runs a profile's own stop command (e.g. `lms server stop`).
pub fn run_stop(p: &Profile) -> Result<(), String> {
    let (bin, args) = p.stop.split_first().ok_or("no stop command")?;
    let st = Command::new(bin).args(args).status().map_err(|e| format!("{bin}: {e}"))?;
    st.success().then_some(()).ok_or_else(|| format!("{bin} exited with {st}"))
}

/// Socket inodes listening on `port` in a `/proc/net/tcp{,6}` table.
pub fn listening_inodes(table: &str, port: u16) -> Vec<u64> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            let local_port = u16::from_str_radix(f.get(1)?.rsplit(':').next()?, 16).ok()?;
            let listening = *f.get(3)? == "0A";
            (listening && local_port == port).then(|| f.get(9)?.parse().ok())?
        })
        .collect()
}

/// PID of the process listening on `port`. Only finds our own processes,
/// since other users' `/proc/<pid>/fd` isn't readable — which is what we want.
pub fn pid_on_port(port: u16) -> Option<u32> {
    let inodes: Vec<u64> = ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .filter_map(|t| fs::read_to_string(t).ok())
        .flat_map(|t| listening_inodes(&t, port))
        .collect();
    if inodes.is_empty() {
        return None;
    }
    let wanted: Vec<String> = inodes.iter().map(|i| format!("socket:[{i}]")).collect();
    fs::read_dir("/proc").ok()?.flatten().find_map(|entry| {
        let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
        let fds = fs::read_dir(entry.path().join("fd")).ok()?;
        fds.flatten()
            .filter_map(|fd| fs::read_link(fd.path()).ok())
            .any(|target| wanted.iter().any(|w| target.as_os_str() == w.as_str()))
            .then_some(pid)
    })
}

pub fn terminate(pid: u32) -> Result<(), String> {
    let st = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map_err(|e| e.to_string())?;
    st.success().then_some(()).ok_or_else(|| format!("kill {pid} failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F91 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 424242 1 0000000000000000 100 0 0 10 0
   1: 0100007F:1F91 0100007F:C350 01 00000000:00000000 00:00000000 00000000  1000        0 515151 1 0000000000000000 20 4 30 10 -1
   2: 0100007F:2BC2 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 777 1 0000000000000000 100 0 0 10 0";

    #[test]
    fn finds_only_listening_socket_on_port() {
        // 0x1F91 = 8081; the ESTABLISHED row on the same port is ignored.
        assert_eq!(listening_inodes(TABLE, 8081), vec![424242]);
        assert_eq!(listening_inodes(TABLE, 11202), vec![777]);
        assert!(listening_inodes(TABLE, 1234).is_empty());
    }
}
