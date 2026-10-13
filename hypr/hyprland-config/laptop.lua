local _, module_path = ...
local dir = (module_path or debug.getinfo(1, "S").source:sub(2)):match("(.*/)")

require(dir .. "shared.lua")

hl.config({
    input = {
        kb_layout = "us, us, de",
        kb_variant = ",dvorak,koy",
        resolve_binds_by_sym = 1,
        follow_mouse = 1,
        touchpad = {
            natural_scroll = true,
            clickfinger_behavior = true,
        },
        sensitivity = 0,
    },
})

for i = 1, 5 do
    hl.workspace_rule({ workspace = tostring(i) })
end

