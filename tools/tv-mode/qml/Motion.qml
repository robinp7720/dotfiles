import QtQuick

// One reversible progress value keeps opacity and translation in step.
QtObject {
    id: motion
    property bool shown: false
    property bool reducedMotion: false
    property int duration: 220
    property int exitDuration: 150
    property real progress: shown ? 1 : 0
    readonly property bool running: animation.running

    function settle() {
        animation.stop();
        progress = shown ? 1 : 0;
    }
    function update() {
        animation.stop();
        if (reducedMotion) {
            settle();
            return;
        }
        animation.from = progress;
        animation.to = shown ? 1 : 0;
        animation.duration = (shown ? duration : exitDuration) * Math.abs(animation.to - progress);
        animation.start();
    }
    function replay() {
        animation.stop();
        progress = reducedMotion ? 1 : 0;
        update();
    }
    Component.onCompleted: settle()
    onShownChanged: update()
    onReducedMotionChanged: if (reducedMotion)
        settle()
    property NumberAnimation animation: NumberAnimation {
        target: motion
        property: "progress"
        easing.type: Easing.OutCubic
    }
}
