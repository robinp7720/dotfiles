// Publishes live workspace-swipe progress on Hyprland's socket2 so bars can track
// trackpad gestures 1:1. Hyprland itself only reports the workspace once a swipe
// commits.
//
//   workspaceswipe>>MONITOR,FROM_ID,TO_ID,PROGRESS
//   workspaceswipeend>>MONITOR
//
// PROGRESS is signed in [-1, 1]: negative moves toward the previous workspace,
// positive toward the next one. TO_ID is empty while PROGRESS is 0.
//
// The plugin hooks nothing. It samples the native swipe state once per rendered
// frame, so it coexists with plugins (hymission, Hyprspace) that hook the swipe.

#include <cmath>
#include <format>
#include <string>
#include <vector>

#include <hyprland/src/config/ConfigValue.hpp>
#include <hyprland/src/desktop/DesktopTypes.hpp>
#include <hyprland/src/desktop/Workspace.hpp>
#include <hyprland/src/event/EventBus.hpp>
#include <hyprland/src/helpers/MiscFunctions.hpp>
#include <hyprland/src/helpers/memory/Memory.hpp>
#include <hyprland/src/managers/EventManager.hpp>
#include <hyprland/src/output/Monitor.hpp>
#include <hyprland/src/plugins/PluginAPI.hpp>

// The swipe state is private and has no accessors. Every dependency of this
// header is already included above, so the override only touches its own body.
#define private public
#include <hyprland/src/managers/input/UnifiedWorkspaceSwipeGesture.hpp>
#undef private

namespace {
    constexpr double PROGRESS_EPSILON = 0.0005;

    struct SSwipeSample {
        std::string monitor;
        WORKSPACEID from     = WORKSPACE_INVALID;
        WORKSPACEID to       = WORKSPACE_INVALID;
        double      progress = 0.0;
    };

    HANDLE                           g_handle = nullptr;
    std::vector<CHyprSignalListener> g_listeners;
    bool                             g_active = false;
    SSwipeSample                     g_last;

    WORKSPACEID neighbourID(double progress) {
        static auto PSWIPEUSER = CConfigValue<Config::INTEGER>("gestures:workspace_swipe_use_r");
        if (progress == 0.0)
            return WORKSPACE_INVALID;
        const auto* relative = progress < 0.0 ? (*PSWIPEUSER ? "r-1" : "m-1") : (*PSWIPEUSER ? "r+1" : "m+1");
        return getWorkspaceIDNameFromString(relative).id;
    }

    void postEnd() {
        if (g_pEventManager)
            g_pEventManager->postEvent(SHyprIPCEvent{"workspaceswipeend", g_last.monitor});
        g_active = false;
        g_last   = {};
    }

    void sample(PHLMONITOR rendered) {
        const auto& swipe = g_pUnifiedWorkspaceSwipe;

        if (!swipe || !swipe->isGestureInProgress()) {
            if (g_active)
                postEnd();
            return;
        }

        const auto monitor = swipe->m_monitor.lock();
        if (!monitor || monitor != rendered)
            return;

        static auto  PSWIPEDIST = CConfigValue<Config::INTEGER>("gestures:workspace_swipe_distance");
        const double distance   = std::max<double>(1.0, *PSWIPEDIST);

        SSwipeSample next;
        next.monitor  = monitor->m_name;
        next.from     = swipe->m_workspaceBegin->m_id;
        next.progress = std::clamp(swipe->m_delta / distance, -1.0, 1.0);
        next.to       = neighbourID(next.progress);

        if (g_active && next.monitor != g_last.monitor)
            postEnd();

        if (g_active && next.from == g_last.from && next.to == g_last.to && std::abs(next.progress - g_last.progress) < PROGRESS_EPSILON)
            return;

        const auto to = next.to == WORKSPACE_INVALID ? std::string{} : std::to_string(next.to);
        if (g_pEventManager)
            g_pEventManager->postEvent(SHyprIPCEvent{"workspaceswipe", std::format("{},{},{},{:.4f}", next.monitor, next.from, to, next.progress)});

        g_active = true;
        g_last   = std::move(next);
    }
}

APICALL EXPORT std::string PLUGIN_API_VERSION() {
    return HYPRLAND_API_VERSION;
}

APICALL EXPORT PLUGIN_DESCRIPTION_INFO PLUGIN_INIT(HANDLE handle) {
    g_handle = handle;

    auto& events = Event::bus()->m_events;
    g_listeners.emplace_back(events.render.pre.listen([](PHLMONITOR monitor) { sample(monitor); }));

    return {"hypr-swipe-events", "Publishes live workspace swipe progress on socket2", "robin", "0.1.0"};
}

APICALL EXPORT void PLUGIN_EXIT() {
    if (g_active)
        postEnd();
    g_listeners.clear();
}
