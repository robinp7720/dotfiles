mod monitor;
mod config;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use monitor::Monitor;
use std::process::Command;

#[derive(Parser)]
#[command(name = "session-manager")]
#[command(about = "Manages desktop sessions based on connected hardware")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List connected monitors and their stable IDs
    List,
    /// Save the current monitor layout as a profile
    Save {
        name: String,
    },
    /// Detect and apply the matching profile
    Apply {
        #[arg(short, long)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => {
            let monitors = monitor::get_connected_monitors()?;
            println!("Connected Monitors:");
            for m in monitors {
                println!("  Interface: {}", m.interface);
                println!("  Description: {}", m.description);
                println!("  Stable ID: {}", m.get_stable_id());
                println!("  Geometry: {}x{} @ {}Hz (Scale: {})", 
                    m.width, m.height, m.refresh_rate as f32 / 1000.0, m.scale.unwrap_or(1.0));
                println!("  Position: {},{}", m.x, m.y);
                if let Some(s) = &m.serial {
                    println!("  Serial: {}", s);
                }
                println!("---");
            }
            let hash = config::generate_hardware_hash(&monitor::get_connected_monitors()?);
            println!("Current Hardware Hash: {}", hash);
        }
        Commands::Save { name } => {
            let mut config = Config::load()?;
            let monitors = monitor::get_connected_monitors()?;
            
            config.add_profile(name.clone(), &monitors);
            config.save()?;
            println!("Saved profile '{}' for current hardware configuration.", name);
        }
        Commands::Apply { dry_run } => {
            let config = Config::load()?;
            let current_monitors = monitor::get_connected_monitors()?;
            
            if let Some(profile) = config.get_profile_for_monitors(&current_monitors) {
                println!("Matched Profile: {}", profile.name);
                apply_profile(profile, &current_monitors, dry_run)?;
            } else {
                eprintln!("No matching profile found for this hardware configuration.");
                eprintln!("Use 'save <NAME>' to learn this configuration.");
            }
        }
    }

    Ok(())
}

fn apply_profile(profile: &config::Profile, current_monitors: &[Monitor], dry_run: bool) -> Result<()> {
    // Map Stable IDs to Current Interfaces
    let mut interface_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    
    for m in current_monitors {
        interface_map.insert(m.get_stable_id(), m.interface.clone());
    }

    if is_niri() {
        apply_niri(profile, &interface_map, dry_run)?;
    } else if is_hyprland() {
        apply_hyprland(profile, &interface_map, dry_run)?;
    } else {
        apply_xrandr(profile, &interface_map, dry_run)?;
    }
    
    // Run custom commands
    if let Some(cmds) = &profile.commands {
        for cmd_str in cmds {
            println!("Running command: {}", cmd_str);
            if !dry_run {
                let parts: Vec<&str> = cmd_str.split_whitespace().collect();
                if !parts.is_empty() {
                    Command::new(parts[0])
                        .args(&parts[1..])
                        .spawn()
                        .context("Failed to spawn command")?;
                }
            }
        }
    }

    Ok(())
}

fn is_niri() -> bool {
    std::env::var("NIRI_SOCKET").is_ok()
}

fn apply_niri(profile: &config::Profile, interface_map: &std::collections::HashMap<String, String>, dry_run: bool) -> Result<()> {
    // niri msg output <OUTPUT> position x y
    // niri msg output <OUTPUT> mode width height
    // niri msg output <OUTPUT> scale <f32>
    // niri msg output <OUTPUT> transform <string>

    for mon_conf in &profile.monitors {
        if let Some(interface) = interface_map.get(&mon_conf.stable_id) {
            let cmds = vec![
                vec!["position".to_string(), "set".to_string(), mon_conf.x.to_string(), mon_conf.y.to_string()],
                // Mode setting in Niri: mode <width>x<height>
                vec!["mode".to_string(), format!("{}x{}", mon_conf.width, mon_conf.height)],
                vec!["scale".to_string(), mon_conf.scale.to_string()],
            ];

            // Transform
             let rotation = match mon_conf.transform {
                 1 => "90",
                 2 => "180",
                 3 => "270",
                 4 => "flipped",
                 5 => "flipped-90", // Check exact string if needed
                 6 => "flipped-180",
                 7 => "flipped-270",
                 _ => "normal",
             };
             
            for mut args in cmds {
                // Prepend common args
                let mut full_args = vec!["msg".to_string(), "output".to_string(), interface.clone()];
                full_args.append(&mut args);
                
                if dry_run {
                    println!("niri {}", full_args.join(" "));
                } else {
                     Command::new("niri").args(&full_args).status()?;
                }
            }
            
            // Apply transform separately
            let transform_args = vec!["msg", "output", interface, "transform", rotation];
            if dry_run {
                println!("niri {}", transform_args.join(" "));
            } else {
                Command::new("niri").args(&transform_args).status()?;
            }

        }
    }
    Ok(())
}

fn is_hyprland() -> bool {
    std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
}

fn apply_hyprland(profile: &config::Profile, interface_map: &std::collections::HashMap<String, String>, dry_run: bool) -> Result<()> {
    // Generate monitors.conf content
    let mut conf_content = String::new();
    conf_content.push_str("# Generated by session-manager\n");

    for mon_conf in &profile.monitors {
        if let Some(interface) = interface_map.get(&mon_conf.stable_id) {
            // monitor=name,resolution,position,scale
            // e.g. monitor=DP-1,1920x1080@144,0x0,1
            let refresh = mon_conf.refresh_rate as f32 / 1000.0;
            // Handle transform if needed (Hyprland uses `monitor=...,transform,X`)
            
            let line = format!("monitor={},{}x{}@{},{}x{},{}", 
                interface, mon_conf.width, mon_conf.height, refresh, mon_conf.x, mon_conf.y, mon_conf.scale);
            
            conf_content.push_str(&line);
            conf_content.push('\n');

            if mon_conf.transform > 0 {
                 conf_content.push_str(&format!("monitor={},transform,{}\n", interface, mon_conf.transform));
            }
        } else {
            eprintln!("Warning: Monitor {} in profile but not connected", mon_conf.stable_id);
        }
    }

    if dry_run {
        println!("---" );
        println!("{}", conf_content);
    } else {
        let mut path = dirs::config_dir().context("No config dir")?;
        path.push("hypr");
        path.push("monitors.conf");
        std::fs::write(&path, conf_content)?;
        
        // Reload Hyprland? Usually it auto-reloads, but we can force it
        // Command::new("hyprctl").arg("reload").status()?; // Often not needed for monitor changes if referenced by source
        // Actually, explicit reload might be good.
    }

    Ok(())
}

fn apply_xrandr(profile: &config::Profile, interface_map: &std::collections::HashMap<String, String>, dry_run: bool) -> Result<()> {
    let mut args = Vec::new();

    for mon_conf in &profile.monitors {
        if let Some(interface) = interface_map.get(&mon_conf.stable_id) {
             args.push("--output".to_string());
             args.push(interface.clone());
             args.push("--mode".to_string());
             args.push(format!("{}x{}", mon_conf.width, mon_conf.height));
             args.push("--pos".to_string());
             args.push(format!("{}x{}", mon_conf.x, mon_conf.y));
             args.push("--rate".to_string());
             // xrandr rate needs to be close
             let rate = format!("{:.2}", mon_conf.refresh_rate as f32 / 1000.0);
             args.push(rate);
             
             if mon_conf.primary {
                 args.push("--primary".to_string());
             }
             
             // Rotation
             let rotation = match mon_conf.transform {
                 1 => "left", // 90 deg
                 2 => "inverted", // 180
                 3 => "right", // 270
                 _ => "normal",
             };
             args.push("--rotate".to_string());
             args.push(rotation.to_string());
        }
    }
    
    // Turn off unused? Not implemented yet for simplicity.

    if dry_run {
        println!("xrandr {}", args.join(" "));
    } else {
        Command::new("xrandr").args(&args).status()?;
    }
    
    Ok(())
}