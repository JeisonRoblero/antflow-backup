use chrono::Utc;
use serde::{Deserialize, Serialize};
use sysinfo::System;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub os_name: String,
    pub os_version: String,
    pub os_edition: String,
    pub hostname: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub cpu_logical_cores: usize,
    pub cpu_arch: String,
    pub cpu_freq_mhz: u64,
    pub ram_mb: u64,
    pub gpu: String,
    pub gpus: Vec<String>,
    pub screen: ScreenInfo,
    pub screens: Vec<ScreenInfo>,
    pub current_datetime_utc: String,
    pub timezone_offset: String,
    pub platform: String,
    pub touch_capable: bool,
}

pub fn collect() -> Result<DeviceInfo, anyhow::Error> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let os_name = System::name().unwrap_or_else(|| "Unknown OS".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "unknown".to_string());
    let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());
    let os_edition = detect_os_edition();

    let cpu_physical = sys.physical_core_count().unwrap_or(0);
    let cpu_logical = sys.cpus().len();
    let cpu_freq_mhz = {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = std::process::Command::new("powershell");
            cmd.args(["-NoProfile", "-Command",
                "(Get-ItemProperty 'HKLM:\\HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0').~MHz"]);
            cmd.creation_flags(0x08000000);
            let out = cmd.output();
            if let Ok(o) = out {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if let Ok(v) = s.parse::<u64>() {
                    v
                } else {
                    let mut cmd2 = std::process::Command::new("powershell");
                    cmd2.args(["-NoProfile", "-Command",
                        "(Get-WmiObject Win32_Processor).MaxClockSpeed"]);
                    cmd2.creation_flags(0x08000000);
                    let out2 = cmd2.output();
                    if let Ok(o2) = out2 {
                        String::from_utf8_lossy(&o2.stdout).trim().parse::<u64>().unwrap_or(0)
                    } else { 0 }
                }
            } else { 0 }
        }
        #[cfg(target_os = "linux")]
        {
            if let Ok(s) = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq") {
                if let Ok(khz) = s.trim().parse::<u64>() {
                    khz / 1000
                } else {
                    let freqs: Vec<u64> = sys.cpus().iter().map(|c| c.frequency()).filter(|&f| f > 0).collect();
                    if freqs.is_empty() { 0 } else { *freqs.iter().max().unwrap_or(&0) }
                }
            } else {
                let freqs: Vec<u64> = sys.cpus().iter().map(|c| c.frequency()).filter(|&f| f > 0).collect();
                if freqs.is_empty() { 0 } else { *freqs.iter().max().unwrap_or(&0) }
            }
        }
        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("sysctl")
                .args(["-n", "hw.cpufrequency_max"])
                .output();
            if let Ok(o) = out {
                let hz = String::from_utf8_lossy(&o.stdout).trim().parse::<u64>().unwrap_or(0);
                hz / 1_000_000
            } else { 0 }
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            let freqs: Vec<u64> = sys.cpus().iter().map(|c| c.frequency()).filter(|&f| f > 0).collect();
            if freqs.is_empty() { 0 } else { *freqs.iter().max().unwrap_or(&0) }
        }
    };

    let cpu_arch = std::env::consts::ARCH.to_string();
    let cpu_model = sys.cpus().first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let ram_mb = sys.total_memory() / 1_048_576;

    let gpu = detect_gpu();
    let gpus = detect_all_gpus();

    let (screen, screens) = capture_all_screens();

    let now = Utc::now();
    let current_datetime_utc = now.to_rfc3339();

    let timezone_offset = chrono::Local::now().offset().to_string();

    let touch_capable = detect_touch();

    Ok(DeviceInfo {
        os_name,
        os_version,
        os_edition,
        hostname,
        cpu_model,
        cpu_cores: cpu_physical,
        cpu_logical_cores: cpu_logical,
        cpu_arch,
        cpu_freq_mhz,
        ram_mb,
        gpu,
        gpus,
        screen,
        screens,
        current_datetime_utc,
        timezone_offset,
        platform: platform.to_string(),
        touch_capable,
    })
}

fn detect_os_edition() -> String {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command",
            "$o = Get-WmiObject Win32_OperatingSystem; $v = (Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion').DisplayVersion; \"$($o.Caption) $v\""]);
        cmd.creation_flags(0x08000000);
        let out = cmd.output();
        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !s.is_empty() { return s; }
        }
    }
    String::new()
}

fn detect_touch() -> bool {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        return true;
    }
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command",
            "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class NativeMethods { [DllImport(\"user32.dll\")] public static extern int GetSystemMetrics(int nIndex); }'; [NativeMethods]::GetSystemMetrics(94)"]);
        cmd.creation_flags(0x08000000);
        let out = cmd.output();
        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if let Ok(v) = s.parse::<i32>() {
                return v & 0x40 != 0 || v & 0x80 != 0;
            }
        }
        let mut cmd2 = std::process::Command::new("powershell");
        cmd2.args(["-NoProfile", "-Command",
            "@(Get-PnpDevice -Class HIDClass -Status OK | Where-Object { $_.FriendlyName -match 'touch|stylus|pen|digitizer' }).Count"]);
        cmd2.creation_flags(0x08000000);
        let out2 = cmd2.output();
        if let Ok(o2) = out2 {
            let s = String::from_utf8_lossy(&o2.stdout).trim().to_string();
            if let Ok(v) = s.parse::<i32>() {
                return v > 0;
            }
        }
    }
    false
}

pub fn capture_all_screens() -> (ScreenInfo, Vec<ScreenInfo>) {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        match screenshots::Screen::all() {
            Ok(screens) if !screens.is_empty() => {
                let all: Vec<ScreenInfo> = screens.iter().enumerate().map(|(i, s)| {
                    let info = s.display_info;
                    ScreenInfo {
                        width: info.width,
                        height: info.height,
                        scale_factor: info.scale_factor as f64,
                        is_primary: i == 0,
                    }
                }).collect();
                let primary = all[0].clone();
                (primary, all)
            }
            _ => {
                let s = ScreenInfo { width: 0, height: 0, scale_factor: 1.0, is_primary: true };
                (s.clone(), vec![s])
            }
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let s = if let Ok((cols, rows)) = crossterm::terminal::size() {
            ScreenInfo { width: cols as u32, height: rows as u32, scale_factor: 1.0, is_primary: true }
        } else {
            ScreenInfo { width: 0, height: 0, scale_factor: 1.0, is_primary: true }
        };
        (s.clone(), vec![s])
    }
}

pub fn detect_gpu() -> String {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if let Ok(out) = std::process::Command::new("getprop")
            .arg("ro.hardware.egl")
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() { return s; }
        }
        return "integrated".to_string();
    }

    #[cfg(target_os = "macos")]
    {
        return "apple-metal".to_string();
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("nvidia-smi");
        cmd.args(["--query-gpu=name", "--format=csv,noheader"]);
        cmd.creation_flags(0x08000000);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !name.is_empty() {
                    return format!("nvidia: {}", name);
                }
            }
        }
        let mut cmd2 = std::process::Command::new("powershell");
        cmd2.args(["-NoProfile", "-Command",
            "(Get-WmiObject Win32_VideoController | Select-Object -First 1).Name"]);
        cmd2.creation_flags(0x08000000);
        let out = cmd2.output();
        if let Ok(o) = out {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !name.is_empty() { return name; }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=name", "--format=csv,noheader"])
            .output()
        {
            if out.status.success() {
                let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !name.is_empty() {
                    return format!("nvidia: {}", name);
                }
            }
        }
        if let Ok(out) = std::process::Command::new("lspci").output() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                let l = line.to_lowercase();
                if l.contains("vga") || l.contains("3d") || l.contains("display") {
                    if let Some(part) = line.splitn(2, ':').nth(1) {
                        return part.trim().to_string();
                    }
                }
            }
        }
    }

    "integrated".to_string()
}

pub fn detect_all_gpus() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-Command",
            "Get-WmiObject Win32_VideoController | ForEach-Object { $_.Name }"]);
        cmd.creation_flags(0x08000000);
        let out = cmd.output();
        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            let gpus: Vec<String> = s.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            if !gpus.is_empty() { return gpus; }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output();
        if let Ok(o) = out {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                if let Some(arr) = v["SPDisplaysDataType"].as_array() {
                    let gpus: Vec<String> = arr.iter()
                        .filter_map(|g| g["sppci_model"].as_str().map(|s| s.to_string()))
                        .collect();
                    if !gpus.is_empty() { return gpus; }
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = std::process::Command::new("lspci").output() {
            let s = String::from_utf8_lossy(&out.stdout);
            let gpus: Vec<String> = s.lines()
                .filter(|l| {
                    let l = l.to_lowercase();
                    l.contains("vga") || l.contains("3d") || l.contains("display")
                })
                .filter_map(|l| l.splitn(2, ':').nth(1)
                    .and_then(|p| p.splitn(2, ':').nth(1))
                    .map(|p| p.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect();
            if !gpus.is_empty() { return gpus; }
        }
    }
    vec![detect_gpu()]
}