//! Host environment detection for reproducibility metadata.

use std::process::Command;

use crate::report::Environment;

pub fn detect() -> Environment {
    Environment {
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        cpu_model: cpu_model(),
        logical_cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
        memory_gb: memory_gb(),
        rustc: command_output("rustc", &["--version"]),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        git_sha: command_output("git", &["rev-parse", "HEAD"]),
        git_dirty: command_output("git", &["status", "--porcelain"]).map(|s| !s.is_empty()),
    }
}

fn command_output(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn cpu_model() -> Option<String> {
    if let Ok(info) = std::fs::read_to_string("/proc/cpuinfo") {
        return info
            .lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_owned());
    }
    command_output("sysctl", &["-n", "machdep.cpu.brand_string"])
}

fn memory_gb() -> Option<f64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: f64 = info
        .lines()
        .find(|l| l.starts_with("MemTotal:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some((kb / 1024.0 / 1024.0 * 10.0).round() / 10.0)
}
