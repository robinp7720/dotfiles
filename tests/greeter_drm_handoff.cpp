// Exercise the patched library's real disconnect/destructor with fake KMS.
// Test-only access to constructors avoids opening a seat, GPU, or display.
#include <bits/stdc++.h>
#include <aquamarine/backend/Backend.hpp>
#include <aquamarine/backend/Session.hpp>
#define private public
#include <aquamarine/backend/DRM.hpp>
#undef private
#include <cassert>

using namespace Aquamarine;
using namespace Hyprutils::Memory;

struct FakeKms : IDRMImplementation {
    int disables = 0;
    bool commit(CSharedPointer<SDRMConnector>, SDRMConnectorCommitData& data) override {
        assert(data.modeset && data.blocking && !data.enabled);
        ++disables;
        return true;
    }
    bool reset() override { return true; }
    bool moveCursor(CSharedPointer<SDRMConnector>, bool) override { return true; }
};

void scenario(bool preserve, bool hotplug) {
    setenv("AQ_PRESERVE_OUTPUTS_ON_EXIT", preserve ? "1" : "0", 1);
    SBackendImplementationOptions options;
    options.backendType = AQ_BACKEND_NULL;
    auto root = CBackend::create({options}, SBackendOptions{});
    root->session = makeShared<CSession>(); // No libseat/libinput handles.
    auto drm = makeShared<CDRMBackend>(root);
    drm->self = drm;
    auto kms = makeShared<FakeKms>();
    drm->impl = kms;
    std::array<bool, 3> destroyed{};
    std::vector<Hyprutils::Signal::CHyprSignalListener> listeners;
    std::vector<CSharedPointer<SDRMConnector>> connectors;
    for (int i = 0; i < 3; ++i) {
        auto connector = makeShared<SDRMConnector>();
        connector->self = connector;
        connector->backend = drm;
        connector->crtc = makeShared<SDRMCRTC>();
        connector->crtc->id = i + 1;
        connector->szName = "test-" + std::to_string(i);
        connector->output = makeShared<CDRMOutput>(connector->szName, drm, connector);
        connector->output->enabledState = true;
        listeners.push_back(connector->output->events.destroy.listen([&, i] { destroyed[i] = true; }));
        drm->connectors.push_back(connector);
        connectors.push_back(connector);
    }
    if (hotplug) {
        for (auto& connector : connectors)
            connector->disconnect();
    }
    drm.reset();
    assert(std::ranges::all_of(destroyed, [](bool value) { return value; }));
    assert(kms->disables == ((hotplug || !preserve) ? 3 : 0));
    for (auto& connector : connectors) {
        assert(!connector->output);
        assert(connector->status == DRM_MODE_DISCONNECTED);
    }
}
int main() {
    scenario(false, false);
    scenario(true, false);
    scenario(true, true);
    unsetenv("AQ_PRESERVE_OUTPUTS_ON_EXIT");
    std::cout << "KMS handoff: stock teardown disables, opt-in exit retains, hotplug still disables; all destroy events delivered.\n";
}
