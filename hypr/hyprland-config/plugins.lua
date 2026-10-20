-- Preferences and autoloading for Hyprland plugins.
local home = assert(os.getenv("HOME"), "HOME is required")
local data_home = os.getenv("XDG_DATA_HOME") or (home .. "/.local/share")
local plugin_dir = data_home .. "/hyprland/plugins/"
local hyprpm_dir = "/var/cache/hyprpm/" .. assert(os.getenv("USER"), "USER is required") .. "/"

local function resolve_hyprspace()
    local custom_path = plugin_dir .. "Hyprspace.so"
    local f = io.open(custom_path, "r")
    if f then
        f:close()
        return custom_path
    end
    return hyprpm_dir .. "Hyprspace/Hyprspace.so"
end

local plugin_paths = {
    resolve_hyprspace(),
    hyprpm_dir .. "hymission/hymission.so",
    -- Live workspace swipe progress for Vigil (tools/hypr-swipe-events).
    plugin_dir .. "hypr-swipe-events.so",
}

for _, path in ipairs(plugin_paths) do
    local f = io.open(path, "r")
    if f then
        f:close()
        hl.plugin.load(path)
    end
end

local settings = {
    hymission = {
        outer_padding_top = 92,
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
        niri_mode = 0,
        niri_scroll_pixels_per_delta = 1.0,
        niri_workspace_scale = 1.0,
        niri_scrolling_preview_gap = 0,
        toggle_switch_mode = 1,
        switch_toggle_auto_next = 1,
        switch_release_key = "Super_L",
        one_workspace_per_row = 0,
        only_active_workspace = 0,
        only_active_monitor = 0,
        show_special = 0,
        workspace_change_keeps_overview = 1,
        hide_hyprbars_during_overview = 0,
        workspace_strip_anchor = "left",
        workspace_strip_empty_mode = "existing",
        workspace_strip_thickness = 160,
        workspace_strip_gap = 24,
        hide_bar_when_strip = 1,
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
    -- Hyprspace registers under plugin.overview
    overview = {
        disableGestures = false,
    },
    hyprexpo = {
        columns = 3,
        gap_size = 10,
        bg_col = "rgb(111111)",
        enable_gesture = true,
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
    if plugin.name == "Hyprspace" and settings.overview then
        hl.config({ plugin = { overview = settings.overview } })
    end
end

-- 1:1 Continuous finger-tracked gestures:
-- 3 fingers VERTICAL:
--   Swipe UP opens hymission (Mission Control) with 1:1 tracking; swipe DOWN closes it.
--   When hymission is closed, swipe DOWN is passed to Hyprspace which tracks 1:1 from the top.
if hl.plugin and hl.plugin.hymission and hl.plugin.hymission.gesture then
    pcall(hl.plugin.hymission.gesture, {
        fingers = 3,
        direction = "vertical",
        action = "hymission:toggle",
        args = "forceall",
    })

    -- 3 fingers HORIZONTAL: hymission-aware 1:1 continuous workspace swipe
    pcall(hl.plugin.hymission.gesture, {
        fingers = 3,
        direction = "horizontal",
        action = "workspace",
    })
end
