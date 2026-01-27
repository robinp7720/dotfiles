use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Monitor {
    pub interface: String, // e.g., DP-1 (Ephemeral)
    pub description: String, // e.g., "Dell Inc. DELL U2719D"
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32, // stored as integer * 1000 (e.g. 59950 for 59.95)
    pub serial: Option<String>,
    pub scale: Option<f32>,
    pub transform: Option<u8>,
    pub x: i32,
    pub y: i32,
    pub active: bool,
}

impl Monitor {
    pub fn get_stable_id(&self) -> String {
        // Create a unique ID that survives port changes
        // Format: "Model-Serial" or "Model-Res-Rate"
        
        let sanitized_desc = self.description.replace(" ", "_").replace("-", "_");
        
        if let Some(s) = &self.serial {
            format!("{}_{}", sanitized_desc, s)
        } else {
            // Fallback if no serial is exposed (common on some laptops or cheap adapters)
            format!("{}_{}x{}_{}", sanitized_desc, self.width, self.height, self.refresh_rate)
        }
    }
}

pub fn get_connected_monitors() -> Result<Vec<Monitor>> {
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "x11".to_string());
    
    // Check for Niri first
    if std::env::var("NIRI_SOCKET").is_ok() {
        return get_niri_monitors();
    }

    // We prioritize Hyprland if we can detect we are in it
    if is_hyprland_running() {
        return get_hyprland_monitors();
    }
    
    match session_type.as_str() {
        "wayland" => {
             // Try Hyprland first
             match get_hyprland_monitors() {
                 Ok(m) => Ok(m),
                 Err(_) => {
                     // Fallback to XWayland/xrandr
                     eprintln!("Warning: hyprctl failed, falling back to xrandr detection.");
                     get_x11_monitors()
                 }
             }
        },
        "x11" | "tty" => get_x11_monitors(),
        _ => get_x11_monitors(),
    }
}

fn is_hyprland_running() -> bool {
    std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
}

// --- Niri Implementation ---

#[derive(Deserialize)]
struct NiriMonitorRaw {
    #[allow(dead_code)]
    name: String,
    make: String,
    model: String,
    serial: Option<String>,
    // modes: Vec<NiriMode>, // Not strictly needed unless we want to validate
    logical: NiriLogical,
    current_mode: usize,
    modes: Vec<NiriMode>,
}

#[derive(Deserialize)]
struct NiriMode {
    width: u32,
    height: u32,
    refresh_rate: u32,
}

#[derive(Deserialize)]
struct NiriLogical {
    x: i32,
    y: i32,
    scale: f32,
    transform: String, // "Normal", "90", etc.
}

fn get_niri_monitors() -> Result<Vec<Monitor>> {
    let output = Command::new("niri")
        .arg("msg")
        .arg("-j")
        .arg("outputs")
        .output()
        .context("Failed to execute niri msg")?;

    if !output.status.success() {
        anyhow::bail!("niri msg failed");
    }

    // Niri returns a HashMap<String, NiriMonitorRaw>
    let raw_map: std::collections::HashMap<String, NiriMonitorRaw> = serde_json::from_slice(&output.stdout)?;
    
    let mut monitors = Vec::new();
    for (interface, raw) in raw_map {
        let current_mode = raw.modes.get(raw.current_mode).ok_or_else(|| anyhow::anyhow!("Invalid current mode index"))?;

        let transform_byte = match raw.logical.transform.as_str() {
            "Normal" => 0,
            "90" => 1,
            "180" => 2,
            "270" => 3,
            "Flipped" => 4, 
            "Flipped90" => 5,
            "Flipped180" => 6,
            "Flipped270" => 7,
            _ => 0,
        };

        monitors.push(Monitor {
            interface,
            description: format!("{} {}", raw.make, raw.model),
            width: current_mode.width,
            height: current_mode.height,
            refresh_rate: current_mode.refresh_rate, // Niri gives "59977" for 59.977Hz
            serial: raw.serial,
            scale: Some(raw.logical.scale),
            transform: Some(transform_byte),
            x: raw.logical.x,
            y: raw.logical.y,
            active: true, // If it's in the list it's likely active
        });
    }

    monitors.sort_by(|a, b| a.interface.cmp(&b.interface));
    Ok(monitors)
}


#[derive(Deserialize)]
struct HyprMonitorRaw {
    name: String,
    description: String,
    width: u32,
    height: u32,
    #[serde(rename = "refreshRate")]
    refresh_rate: f32,
    x: i32,
    y: i32,
    active: bool,
    scale: f32,
    transform: u8,
    serial: Option<String>, 
    // model: Option<String>,
}

fn get_hyprland_monitors() -> Result<Vec<Monitor>> {
    let output = Command::new("hyprctl")
        .arg("monitors")
        .arg("all")
        .arg("-j")
        .output()
        .context("Failed to execute hyprctl")?;

    if !output.status.success() {
        anyhow::bail!("hyprctl failed");
    }

    let raw_monitors: Vec<HyprMonitorRaw> = serde_json::from_slice(&output.stdout)?;
    
    let mut monitors = Vec::new();
    for raw in raw_monitors {
        monitors.push(Monitor {
            interface: raw.name,
            description: raw.description, 
            width: raw.width,
            height: raw.height,
            refresh_rate: (raw.refresh_rate * 1000.0) as u32,
            serial: raw.serial, 
            scale: Some(raw.scale),
            transform: Some(raw.transform),
            x: raw.x,
            y: raw.y,
            active: raw.active,
        });
    }

    monitors.sort_by(|a, b| a.interface.cmp(&b.interface));
    Ok(monitors)
}

fn get_x11_monitors() -> Result<Vec<Monitor>> {
    // Parsing `xrandr --verbose` is painful but necessary for Serial Numbers.
    // For now, to keep it simple, we will use `xrandr --listmonitors` for basics
    // and might need `xrandr --verbose` if we strictly need serials.
    // Let's try `xrandr --prop` (properties) which usually has EDID.
    
    let output = Command::new("xrandr")
        .arg("--prop")
        .output()
        .context("Failed to execute xrandr")?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    
    // Primitive parser for xrandr
    // This is a placeholder. A robust parser is complex.
    // We will look for "connected" lines.
    
    let mut monitors = Vec::new();
    let mut current_monitor: Option<Monitor> = None;
    
    for line in output_str.lines() {
        if line.contains(" connected ") {
            // e.g. "DP-1 connected primary 1920x1080+0+0 ..."
            if let Some(m) = current_monitor.take() {
                monitors.push(m);
            }
            
            let parts: Vec<&str> = line.split_whitespace().collect();
            let interface = parts[0].to_string();
            
            // Check if active (has resolution like 1920x1080+0+0)
            let active = line.contains("+");
            
            // Try to extract resolution
            let (w, h, x, y) = if let Some(res_str) = parts.iter().find(|p| p.contains("x") && p.contains("+")) {
                 // 1920x1080+0+0
                 parse_xrandr_geometry(res_str).unwrap_or((0,0,0,0))
            } else {
                (0,0,0,0)
            };

            current_monitor = Some(Monitor {
                interface,
                description: "Unknown X11 Display".to_string(), // xrandr often hides model in EDID
                width: w,
                height: h,
                refresh_rate: 60000, // Default fallback
                serial: None,
                scale: Some(1.0),
                transform: None,
                x,
                y,
                active,
            });
        } else if let Some(_) = current_monitor {
            // Try to find EDID or other props
            // This is where we would parse EDID to get the real name/serial
            // For now, we leave it simple.
            if line.trim().starts_with("EDID:") {
                // Parsing EDID hex is needed for true identity in X11
            }
        }
    }
    
    if let Some(m) = current_monitor {
        monitors.push(m);
    }
    
    Ok(monitors)
}

fn parse_xrandr_geometry(s: &str) -> Option<(u32, u32, i32, i32)> {
    // 1920x1080+0+0
    let parts: Vec<&str> = s.split(|c| c == 'x' || c == '+').collect();
    if parts.len() < 4 { return None; }
    
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
        parts[3].parse().ok()?,
    ))
}
