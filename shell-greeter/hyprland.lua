-- Dedicated pre-login compositor: no desktop startup, bindings, or plugins.
-- setup installs the generated monitor snapshot beside this file.
require("./monitors.lua")
hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
hl.env("XCURSOR_SIZE", "24")
hl.env("XCURSOR_THEME", "capitaine-cursors")
hl.config({
    misc = { disable_hyprland_logo = true, disable_splash_rendering = true },
    cursor = { no_hardware_cursors = 1 },
    animations = { enabled = false },
    decoration = { rounding = 0, shadow = { enabled = false }, blur = { enabled = false } },
})
hl.on("hyprland.start", function()
    hl.exec_cmd("/usr/local/libexec/shell-greeter-session")
end)
