-- Preferences and autoloading for Hyprland plugins.
local home = assert(os.getenv("HOME"), "HOME is required")
local data_home = os.getenv("XDG_DATA_HOME") or (home .. "/.local/share")
local plugin_dir = data_home .. "/hyprland/plugins/"
local hyprpm_dir = "/var/cache/hyprpm/" .. assert(os.getenv("USER"), "USER is required") .. "/"

local function read_file(path)
    local file = io.open(path, "r")
    if not file then return nil end
    local contents = file:read("*a")
    file:close()
    return contents
end

-- Check before dlopen: incompatible C++ static initializers can crash before
-- a plugin's own version check runs. Use this process's executable so reloading
-- after a package upgrade still checks against the running compositor.
local pid = (read_file("/proc/self/stat") or ""):match("^(%d+)")
local version = pid and io.popen("/proc/" .. pid .. "/exe --version 2>/dev/null", "r")
local abi = version and version:read("*a"):match("Version ABI string: (%S+)")
if version then version:close() end
local hyprpm_abi = (read_file(hyprpm_dir .. "state.toml") or ""):match("\nhash%s*=%s*['\"]([^'\"]+)['\"]")

local function compatible(path)
    local file = io.open(path, "r")
    if not file then return false end
    file:close()
    local built_abi
    if path:sub(1, #hyprpm_dir) == hyprpm_dir then
        built_abi = hyprpm_abi
    else
        built_abi = (read_file(path .. ".abi") or ""):match("^(%S+)")
    end
    if abi and built_abi == abi then return true end
    print("Skipping incompatible or unverified plugin: " .. path .. "; rebuild it for this Hyprland version")
    return false
end

local function resolve_plugin(name)
    local custom_path = plugin_dir .. name .. ".so"
    if compatible(custom_path) then
        return custom_path
    end
    return hyprpm_dir .. name .. "/" .. name .. ".so"
end

local plugin_paths = {
    resolve_plugin("hymission"),
    -- Live workspace swipe progress for Vigil (tools/hypr-swipe-events).
    plugin_dir .. "hypr-swipe-events.so",
}

for _, path in ipairs(plugin_paths) do
    if compatible(path) then
        hl.plugin.load(path)
    end
end

local settings = {
    hymission = {
        outer_padding_top = 32,
        outer_padding_right = 32,
        outer_padding_bottom = 32,
        outer_padding_left = 32,
        row_spacing = 32,
        column_spacing = 32,
        min_window_length = 120,
        min_preview_short_edge = 32,
        small_window_boost = 1.35,
        max_preview_scale = 0.95,
        workspace_overview_max_preview_scale = 0.95,
        min_slot_scale = 0.10,
        natural_scale_flex = 0.22,
        layout_engine = "grid",
        layout_scale_weight = 1.0,
        layout_space_weight = 0.10,
        expand_selected_window = 1,
        overview_focus_follows_mouse = 1,
        multi_workspace_sort_recent_first = 1,
        -- Swipe up for the current workspace, down for the whole monitor.
        gesture_invert_vertical = 1,
        gesture_close_restores_focus = 1,
        niri_mode = 0,
        niri_scroll_pixels_per_delta = 1.0,
        niri_workspace_scale = 1.0,
        niri_scrolling_preview_gap = 0,
        toggle_switch_mode = 1,
        switch_toggle_auto_next = 1,
        switch_release_key = "Super_L",
        one_workspace_per_row = 0,
        only_active_workspace = 0,
        only_active_monitor = 1,
        show_special = 1,
        workspace_change_keeps_overview = 1,
        hide_hyprbars_during_overview = 0,
        -- Workspaces line up along the top, like Spaces in Mission Control.
        workspace_strip_anchor = "top",
        workspace_strip_empty_mode = "existing",
        workspace_strip_thickness = 160,
        workspace_strip_gap = 24,
        -- Keep Vigil visible; the strip already respects its reserved top edge.
        hide_bar_when_strip = 0,
        hide_bar_animation = 1,
        hide_bar_animation_blur = 1,
        hide_bar_animation_move_multiplier = 0.8,
        hide_bar_animation_scale_divisor = 1.1,
        hide_bar_animation_alpha_end = 0,
        bar_single_mission_control = 0,
        show_focus_indicator = 0,
        backdrop_blur = 0,
        backdrop_color = "rgba(00000000)",
        focus_hover_color = "rgba(f2f7ff8c)",
        focus_selected_color = "rgba(3dc7fff2)",
        focus_hover_thickness = 2,
        focus_selected_thickness = 4,
        workspace_strip_inactive_tint_color = "rgba(00000000)",
        debug_logs = 0,
        debug_surface_logs = 0,
    },
    hyprscrolling = {
        column_width = 0.7,
        fullscreen_on_one_column = false,
    },
}

-- Apply plugin configs
for _, plugin in ipairs(hl.get_loaded_plugins()) do
    if settings[plugin.name] then
        hl.config({ plugin = { [plugin.name] = settings[plugin.name] } })
    end
end

-- Mission Control on three-finger vertical swipes, tracking the fingers 1:1:
--   up   -> windows on the current workspace
--   down -> windows from every workspace on the monitor under the cursor
-- Swiping the opposite way closes the overview; keep going to switch views.
-- `recommand` names its wide side `forceall`, which spans every monitor unless
-- hymission is the tools/hymission fork. The fork also lets the overview drop
-- its window search.
local hymission = hl.plugin and hl.plugin.hymission
if hymission and hymission.gesture then
    -- Probe with get_config: an unknown key passed to hl.config is reported as
    -- a config error even when the call is protected.
    local function set_if_known(key, value)
        if hl.get_config("plugin.hymission." .. key) == nil then return false end
        hl.config({ plugin = { hymission = { [key] = value } } })
        return true
    end
    set_if_known("search_enabled", 0)
    if set_if_known("forceall_only_active_monitor", 1) then
        hymission.gesture({
            fingers = 3,
            direction = "vertical",
            action = "toggle",
            recommand = true,
        })
    else
        -- Upstream hymission: keep the scopes, but open without finger tracking.
        hl.gesture({
            fingers = 3,
            direction = "up",
            action = function() hymission.open("onlycurrentworkspace") end,
        })
        hl.gesture({
            fingers = 3,
            direction = "down",
            action = function() hymission.open() end,
        })
    end

    -- Workspace swipes that also move between workspaces inside the overview.
    pcall(hymission.gesture, {
        fingers = 3,
        direction = "horizontal",
        action = "workspace",
    })
end
