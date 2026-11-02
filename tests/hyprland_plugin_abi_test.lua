-- Run with: lua tests/hyprland_plugin_abi_test.lua
local root = arg[1] or "."
local real_open, real_popen, real_getenv, real_print = io.open, io.popen, os.getenv, print
local source_file = assert(real_open(root .. "/hypr/hyprland-config/plugins.lua"))
local source = source_file:read("*a")
source_file:close()
local local_dir = "/test/data/hyprland/plugins/"
local cache = "/var/cache/hyprpm/test/"
local current = "current_aq_0.16_hu_0.14_hg_0.6_hc_0.1_hlg_0.6"

local function run(files, version)
    local loaded = {}
    local function file(contents)
        return {read = function() return contents end, close = function() end}
    end
    io.open = function(path)
        if path == "/proc/self/stat" then return file("123 (Hyprland)") end
        if files[path] then return file(files[path]) end
    end
    io.popen = function(command)
        assert(command == "/proc/123/exe --version 2>/dev/null")
        return version and file("Version ABI string: " .. version .. "\n") or nil
    end
    os.getenv = function(key)
        return ({HOME = "/test", USER = "test", XDG_DATA_HOME = "/test/data"})[key]
    end
    print = function() end
    hl = {
        plugin = {load = function(path) table.insert(loaded, path) end},
        get_loaded_plugins = function() return {} end,
    }
    assert(load(source, "plugins.lua"))()
    return table.concat(loaded, "\n")
end

local files = {
    [local_dir .. "Hyprspace.so"] = "binary",
    [local_dir .. "hypr-swipe-events.so"] = "binary",
    [cache .. "Hyprspace/Hyprspace.so"] = "binary",
    [cache .. "hymission/hymission.so"] = "binary",
    [cache .. "state.toml"] = "[state]\nhash = 'old'",
}
assert(run(files, current) == "", "unverified local and stale cached binaries must be skipped")
files[local_dir .. "Hyprspace.so.abi"] = "old\n"
assert(run(files, current) == "", "stale local ABI must be skipped")
files[cache .. "state.toml"] = '[state]\nhash = "' .. current .. '"'
assert(run(files, current) == cache .. "Hyprspace/Hyprspace.so\n" .. cache .. "hymission/hymission.so",
    "stale local build must fall back to compatible cache")
files[local_dir .. "Hyprspace.so.abi"] = current .. "\n"
files[local_dir .. "hypr-swipe-events.so.abi"] = current .. "\n"
assert(run(files, current) == local_dir .. "Hyprspace.so\n" .. cache .. "hymission/hymission.so\n" .. local_dir .. "hypr-swipe-events.so",
    "compatible local plugins must load and take precedence")
assert(run(files, nil) == "", "unknown running ABI must fail closed")
assert(run({}, current) == "", "clean installation must work without plugins")
io.open, io.popen, os.getenv, print = real_open, real_popen, real_getenv, real_print
print("Hyprland plugin ABI compatibility checks passed.")
