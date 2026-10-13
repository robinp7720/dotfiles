-- setup.sh installs this config and its dependencies together in /etc/nwg-hello.
require("./base.lua")
hl.env("AQ_DRM_DEVICES", "/dev/dri/card2:/dev/dri/card1")
require("./monitors.lua")

hl.bind("ALT + Q", hl.dsp.window.close())
hl.config({
    misc = { disable_hyprland_logo = true },
    cursor = { no_hardware_cursors = 1 },
})

hl.on("hyprland.start", function()
    hl.exec_cmd("nwg-hello; hyprctl dispatch 'hl.dsp.exit()'")
end)
