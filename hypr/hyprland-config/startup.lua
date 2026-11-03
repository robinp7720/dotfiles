local _, module_path = ...
local dir = (module_path or debug.getinfo(1, "S").source:sub(2)):match("(.*/)")

hl.env("SSH_AUTH_SOCK", assert(os.getenv("XDG_RUNTIME_DIR"), "XDG_RUNTIME_DIR is required") .. "/gcr/ssh")

-- nwg-displays and the mode scripts write this watched Lua module.
local monitors = io.open(dir .. "../monitors.lua", "r")
if monitors then
    monitors:close()
    require(dir .. "../monitors.lua")
else
    hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
end

hl.on("hyprland.start", function()
    hl.exec_cmd("~/.dotfiles/scripts/start_desktop_services.sh hyprland")
    hl.exec_cmd("~/.dotfiles/scripts/album_art_watcher.sh &")
    hl.exec_cmd("setsid -f ~/.dotfiles/scripts/center_tws_login.sh")
    hl.exec_cmd("firefox")
    hl.exec_cmd("/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1")
    hl.exec_cmd("/usr/bin/kdeconnectd")
    hl.exec_cmd("/usr/bin/hyprsunset")
    hl.exec_cmd("~/.dotfiles/scripts/start_codex_self_improve_service.sh")
end)
