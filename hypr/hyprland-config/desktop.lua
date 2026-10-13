local _, module_path = ...
local dir = (module_path or debug.getinfo(1, "S").source:sub(2)):match("(.*/)")

hl.env("WEBKIT_DMABUF_RENDERER_DISABLE_GBM", "1")
require(dir .. "shared.lua")

hl.config({
    input = {
        kb_layout = "us, us, de",
        kb_variant = ",dvorak,koy",
        kb_model = "",
        kb_rules = "",
        follow_mouse = 1,
        touchpad = {
            natural_scroll = false,
        },
        emulate_discrete_scroll = 0,
        scroll_factor = 1.0,
        sensitivity = 0,
    },
})

hl.device({ name = "logitech-usb-receiver-mouse", scroll_factor = 0.125 })
hl.device({ name = "logitech-usb-receiver-1", scroll_factor = 0.125 })
hl.device({ name = "logitech-usb-receiver-consumer-control-1", scroll_factor = 0.125 })
