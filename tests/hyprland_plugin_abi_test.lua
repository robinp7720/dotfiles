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
    [local_dir .. "hymission.so"] = "binary",
    [local_dir .. "hypr-swipe-events.so"] = "binary",
    [cache .. "hymission/hymission.so"] = "binary",
    [cache .. "state.toml"] = "[state]\nhash = 'old'",
}
assert(run(files, current) == "", "unverified local and stale cached binaries must be skipped")
files[local_dir .. "hymission.so.abi"] = "old\n"
assert(run(files, current) == "", "stale local ABI must be skipped")
files[cache .. "state.toml"] = '[state]\nhash = "' .. current .. '"'
assert(run(files, current) == cache .. "hymission/hymission.so",
    "stale local build must fall back to compatible cache")
files[local_dir .. "hymission.so.abi"] = current .. "\n"
files[local_dir .. "hypr-swipe-events.so.abi"] = current .. "\n"
assert(run(files, current) == local_dir .. "hymission.so\n" .. local_dir .. "hypr-swipe-events.so",
    "compatible local plugins must load and take precedence")
assert(run(files, nil) == "", "unknown running ABI must fail closed")
assert(run({}, current) == "", "clean installation must work without plugins")

-- Overview gestures: up shows the current workspace, down the whole monitor.
local function gestures(patched)
    local native, plugin, config = {}, {}, {}
    io.open = function() end
    io.popen = function() end
    os.getenv = function(key) return ({HOME = "/test", USER = "test"})[key] end
    print = function() end
    hl = {
        plugin = {
            load = function() end,
            hymission = {
                gesture = function(spec) table.insert(plugin, spec) end,
                open = function(scope) table.insert(config, {opened = scope or "default"}) end,
            },
        },
        get_loaded_plugins = function() return {{name = "hymission"}} end,
        -- Like Hyprland, report unknown keys as config errors even under pcall.
        config = function(values)
            local options = values.plugin.hymission
            if (options.forceall_only_active_monitor ~= nil or options.search_enabled ~= nil) and not patched then
                table.insert(config, {error = "unknown hymission option"})
                error("unknown hymission option")
            end
            for key, value in pairs(options) do config[key] = value end
        end,
        get_config = function(key)
            if key == "plugin.hymission.forceall_only_active_monitor" or key == "plugin.hymission.search_enabled" then
                return patched and 0 or nil
            end
        end,
        gesture = function(spec) table.insert(native, spec) end,
    }
    assert(load(source, "plugins.lua"))()
    return native, plugin, config
end

local native, plugin, config = gestures(true)
assert(#native == 0, "forked hymission must not register discrete overview swipes")
assert(config.gesture_invert_vertical == 1, "swiping up must open the current-workspace side")
assert(config.forceall_only_active_monitor == 1, "swiping down must stay on the cursor's monitor")
assert(config.only_active_monitor == 1, "default overview scope must stay on one monitor")
assert(config.search_enabled == 0, "the overview must not offer window search")
assert(config.workspace_strip_anchor == "top", "workspaces must line up along the top")
assert(#plugin == 2 and plugin[1].direction == "vertical" and plugin[1].action == "toggle"
    and plugin[1].recommand == true and plugin[1].fingers == 3,
    "vertical swipes must use the finger-tracked two-sided toggle")
assert(plugin[2].direction == "horizontal" and plugin[2].action == "workspace",
    "horizontal swipes must switch workspaces through hymission")

native, plugin, config = gestures(false)
assert(#plugin == 1 and plugin[1].direction == "horizontal",
    "upstream hymission must not use the all-monitor two-sided toggle")
assert(#native == 2, "upstream hymission must fall back to discrete swipes")
local by_direction = {}
for _, spec in ipairs(native) do by_direction[spec.direction] = spec end
by_direction.up.action()
by_direction.down.action()
assert(not config[1].error, "probing an upstream hymission must not raise a config error")
assert(config[1].opened == "onlycurrentworkspace" and config[2].opened == "default",
    "discrete swipes must open the current workspace (up) and the monitor (down)")

io.open, io.popen, os.getenv, print = real_open, real_popen, real_getenv, real_print
print("Hyprland plugin ABI and overview gesture checks passed.")
