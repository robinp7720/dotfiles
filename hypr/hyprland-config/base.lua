-- Shared compositor settings for the desktop and greeter.
hl.env("XDG_SESSION_TYPE", "wayland")
hl.env("XDG_CURRENT_DESKTOP", "Hyprland")
hl.env("QT_QPA_PLATFORM", "wayland")
hl.env("XCURSOR_SIZE", "24")
hl.env("XCURSOR_THEME", "capitaine-cursors")

hl.config({
    general = {
        gaps_in = 5,
        gaps_out = 5,
        border_size = 2,
        ["col.active_border"] = { colors = { "rgba(33ccffee)", "rgba(00ff99ee)" }, angle = 45 },
        ["col.inactive_border"] = "rgba(59595966)",
        resize_on_border = true,
        extend_border_grab_area = 10,
        allow_tearing = true,
        layout = "dwindle",
    },
    misc = {
        allow_session_lock_restore = true,
    },
    animations = {
        enabled = true,
    },
    decoration = {
        rounding = 14,
        shadow = {
            enabled = true,
            range = 24,
            render_power = 3,
            color = "rgba(00000066)",
            offset = { 0, 8 },
            scale = 0.95,
        },
        dim_around = 0.7,
        inactive_opacity = 1,
        active_opacity = 1,
        blur = {
            ignore_opacity = false,
            new_optimizations = true,
            enabled = true,
            size = 7,
            xray = false,
            passes = 3,
            popups = true,
            special = true,
        },
    },
    dwindle = {
        preserve_split = true,
    },
    master = {
        allow_small_split = true,
    },
    debug = {
        vfr = true,
        disable_logs = false,
        enable_stdout_logs = true,
    },
})

-- Durations are deciseconds; see matugen/UI_STYLE.md for the shared motion scale.
hl.curve("spring", { type = "bezier", points = { { 0.16, 1 }, { 0.30, 1.16 } } })
hl.curve("settle", { type = "bezier", points = { { 0.16, 1 }, { 0.30, 1 } } })
hl.curve("leave", { type = "bezier", points = { { 0.55, 0 }, { 0.80, 0.30 } } })
hl.animation({ leaf = "windowsIn", enabled = true, speed = 3.2, bezier = "spring", style = "popin 92%" })
hl.animation({ leaf = "windowsOut", enabled = true, speed = 1.8, bezier = "leave", style = "popin 97%" })
hl.animation({ leaf = "windowsMove", enabled = true, speed = 2.8, bezier = "settle" })
hl.animation({ leaf = "workspaces", enabled = true, speed = 3.2, bezier = "settle", style = "slidefade 12%" })
hl.animation({ leaf = "specialWorkspace", enabled = true, speed = 3.2, bezier = "spring", style = "slidefadevert 10%" })
hl.animation({ leaf = "layersIn", enabled = true, speed = 3.2, bezier = "spring", style = "popin 92%" })
hl.animation({ leaf = "layersOut", enabled = true, speed = 1.8, bezier = "leave", style = "popin 98%" })
hl.animation({ leaf = "fade", enabled = true, speed = 1.8, bezier = "settle" })
hl.animation({ leaf = "fadeOut", enabled = true, speed = 1.4, bezier = "leave" })
hl.animation({ leaf = "border", enabled = true, speed = 1.8, bezier = "settle" })
hl.animation({ leaf = "borderangle", enabled = false })
hl.layer_rule({ name = "Luma-blur", match = { namespace = "Luma" }, blur = true })
hl.layer_rule({ name = "kitty-quick-access-blur", match = { namespace = "kitty-quick-access" }, blur = true })
hl.layer_rule({ name = "vigil-blur", match = { namespace = "vigil" }, blur = true })
-- Vigil owns the bar and quick-settings entrance animations.
hl.layer_rule({ name = "vigil-no_anim", match = { namespace = "vigil" }, no_anim = true })
hl.layer_rule({ name = "vigil-control-center-no_anim", match = { namespace = "vigil-control-center" }, no_anim = true })
hl.layer_rule({ name = "tv-mode-no_anim", match = { namespace = "tv-mode" }, no_anim = true })
hl.layer_rule({ name = "vigil-control-center-blur", match = { namespace = "vigil-control-center" }, blur = true })
hl.layer_rule({ name = "vigil-notifications-blur", match = { namespace = "vigil-notifications" }, blur = true })
hl.layer_rule({ name = "kitty-quick-access-animation", match = { namespace = "kitty-quick-access" }, animation = "slide top" })
hl.layer_rule({ name = "Luma-animation", match = { namespace = "Luma" }, animation = "popin 92%" })
hl.layer_rule({ name = "Luma-ignore_alpha", match = { namespace = "Luma" }, ignore_alpha = 0.45 })
hl.layer_rule({ name = "vigil-ignore_alpha", match = { namespace = "vigil" }, ignore_alpha = 0.20 })
hl.layer_rule({ name = "vigil-control-center-ignore_alpha", match = { namespace = "vigil-control-center" }, ignore_alpha = 0.20 })
hl.layer_rule({ name = "vigil-notifications-ignore_alpha", match = { namespace = "vigil-notifications" }, ignore_alpha = 0.20 })

hl.gesture({ fingers = 3, direction = "horizontal", action = "workspace" })
