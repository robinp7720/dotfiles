-- Set HYPRLAND_PROFILE=laptop before starting Hyprland for the laptop settings.
local profile = os.getenv("HYPRLAND_PROFILE") or "desktop"
assert(profile == "desktop" or profile == "laptop", "Unknown HYPRLAND_PROFILE: " .. profile)
require("./hyprland-config/" .. profile .. ".lua")
