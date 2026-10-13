local _, module_path = ...
local dir = (module_path or debug.getinfo(1, "S").source:sub(2)):match("(.*/)")

require(dir .. "base.lua")
require(dir .. "startup.lua")
require(dir .. "bindings.lua")
require(dir .. "plugins.lua")

-- A clean checkout can start before Matugen has generated a palette.
local colors = io.open(dir .. "../colors.lua", "r")
if colors then
    colors:close()
    require(dir .. "../colors.lua")
end
