use std::process::Command;
use std::thread;
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub const PROCESS_CANDIDATES: &[&str] = &["Elgato.WaveLink", "WaveLink", "WavelinkSEService"];
pub const BLOCKING_PROCESS_NAMES: &[&str] = &["Elgato.WaveLink", "WaveLink"];

fn suppress_console_window(command: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub fn running_wavelink_processes() -> Result<Vec<String>, String> {
    if cfg!(target_os = "windows") {
        windows_running_processes()
    } else if cfg!(target_os = "macos") {
        macos_running_processes()
    } else {
        Ok(vec![])
    }
}

pub fn terminate_wavelink_processes() -> Result<Vec<String>, String> {
    let running = running_wavelink_processes()?;
    if running.is_empty() {
        return Ok(vec![]);
    }

    if cfg!(target_os = "windows") {
        for name in PROCESS_CANDIDATES {
            let mut cmd_exe = Command::new("taskkill");
            cmd_exe.args(["/IM", &format!("{name}.exe"), "/F"]);
            let _ = suppress_console_window(&mut cmd_exe).output();

            let mut cmd_plain = Command::new("taskkill");
            cmd_plain.args(["/IM", name, "/F"]);
            let _ = suppress_console_window(&mut cmd_plain).output();
        }
    } else if cfg!(target_os = "macos") {
        for name in PROCESS_CANDIDATES {
            let _ = Command::new("pkill").args(["-f", name]).output();
        }
    }

    // Give the OS a brief moment to retire processes before the next check.
    thread::sleep(Duration::from_millis(350));

    Ok(running)
}

pub fn launch_wavelink() -> Result<(), String> {
    if cfg!(target_os = "windows") {
        let mut cmd = Command::new("explorer");
        cmd.arg("shell:AppsFolder\\Elgato.WaveLink_g54w8ztgkx496!App");
        let status = suppress_console_window(&mut cmd)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Failed to launch Wave Link via AppsFolder".to_string());
        }
        return Ok(());
    }

    if cfg!(target_os = "macos") {
        let status = Command::new("open")
            .args(["-a", "Wave Link"])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Failed to launch Wave Link via open -a".to_string());
        }
    }

    Ok(())
}

pub fn filter_blocking_processes(processes: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for process in processes {
        if BLOCKING_PROCESS_NAMES
            .iter()
            .any(|name| process.eq_ignore_ascii_case(name))
        {
            out.push(process.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn windows_running_processes() -> Result<Vec<String>, String> {
    let mut cmd = Command::new("tasklist");
    cmd.args(["/FO", "CSV", "/NH"]);
    let output = suppress_console_window(&mut cmd)
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut image_names = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // CSV row starts with quoted image name: "name.exe",...
        let Some(first_quote) = line.find('"') else {
            continue;
        };
        let rest = &line[first_quote + 1..];
        let Some(end_quote) = rest.find('"') else {
            continue;
        };
        image_names.push(rest[..end_quote].to_lowercase());
    }

    let mut found = Vec::new();
    for image in image_names {
        match image.as_str() {
            "elgato.wavelink.exe" => found.push("Elgato.WaveLink".to_string()),
            "wavelink.exe" => found.push("WaveLink".to_string()),
            "wavelinkseservice.exe" => found.push("WavelinkSEService".to_string()),
            _ => {}
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}

fn macos_running_processes() -> Result<Vec<String>, String> {
    let output = Command::new("ps")
        .args(["-ax"])
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();

    let mut found = Vec::new();
    for candidate in PROCESS_CANDIDATES {
        if text.contains(&candidate.to_lowercase()) {
            found.push((*candidate).to_string());
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}
