import QtQuick
import QtQuick.Controls

Item {
    id: player
    property var playback: ({})
    property string title: ""
    property string notice: ""
    property color accent: "#83d2e3"
    property bool reducedMotion: false
    property bool controlsVisible: true
    property bool browsingControls: false
    property int controlIndex: 0
    property string menu: ""
    property var menuEntries: []
    property int menuIndex: 0
    readonly property bool paused: !!playback.paused
    readonly property bool buffering: !!playback.buffering
    signal action(var value)
    function command(name, value) {
        activity();
        action({
            type: "player",
            command: name,
            value: value === undefined ? 0 : value
        });
    }
    function time(seconds) {
        let value = Math.max(0, Math.floor(seconds || 0));
        let hours = Math.floor(value / 3600), minutes = Math.floor(value / 60) % 60;
        return (hours ? hours + ":" + String(minutes).padStart(2, "0") : minutes) + ":" + String(value % 60).padStart(2, "0");
    }
    function activity() {
        controlsVisible = true;
        hideTimer.restart();
    }
    function focusControl() {
        if (browsingControls && controls.itemAt(controlIndex))
            controls.itemAt(controlIndex).forceActiveFocus();
        else
            player.forceActiveFocus();
    }
    function openMenu(kind) {
        menu = kind;
        if (kind === "Audio")
            menuEntries = (playback.audioTracks || []).map(t => ({
                        label: t.label,
                        value: t.index,
                        selected: t.index === playback.selectedAudio
                    }));
        else if (kind === "Subtitles")
            menuEntries = [
                {
                    label: "Off",
                    value: -1,
                    selected: playback.selectedSubtitle === null
                }
            ].concat((playback.subtitleTracks || []).map(t => ({
                        label: t.label,
                        value: t.index,
                        selected: t.index === playback.selectedSubtitle
                    })));
        else
            menuEntries = [
                {
                    label: "Original",
                    value: 0
                },
                {
                    label: "Auto",
                    value: 1
                },
                {
                    label: "1080p",
                    value: 1080
                },
                {
                    label: "720p",
                    value: 720
                },
                {
                    label: "480p",
                    value: 480
                },
                {
                    label: "360p",
                    value: 360
                }
            ].map(t => ({
                        label: t.label,
                        value: t.value,
                        selected: playback.quality === (t.value === 0 ? "original" : t.value === 1 ? "auto" : t.value)
                    }));
        menuIndex = Math.max(0, menuEntries.findIndex(t => t.selected));
        menuList.positionViewAtIndex(menuIndex, ListView.Contain);
        forceActiveFocus();
        activity();
    }
    function selectMenu(index) {
        if (!menuEntries[index])
            return;
        command(menu === "Audio" ? "audioTrack" : menu === "Subtitles" ? "subtitleTrack" : "setQuality", menuEntries[index].value);
        menu = "";
        focusControl();
    }
    function navigate(key) {
        if (key === "release")
            return;
        if (key === "playerActivity") {
            activity();
            return;
        }
        activity();
        if (menu) {
            if (key === "back") {
                menu = "";
                focusControl();
            } else if (key === "up" || key === "down") {
                menuIndex = Math.max(0, Math.min(menuEntries.length - 1, menuIndex + (key === "down" ? 1 : -1)));
                menuList.positionViewAtIndex(menuIndex, ListView.Contain);
            } else if (key === "press")
                selectMenu(menuIndex);
            return;
        }
        if (key === "back") {
            command("stop");
            return;
        }
        if (key === "down") {
            browsingControls = true;
            focusControl();
            return;
        }
        if (key === "up") {
            browsingControls = false;
            focusControl();
            return;
        }
        if (key === "left" || key === "right") {
            if (browsingControls) {
                controlIndex = Math.max(0, Math.min(controls.count - 1, controlIndex + (key === "right" ? 1 : -1)));
                focusControl();
            } else
                command("seek", key === "right" ? 10 : -10);
        } else if (key === "press") {
            if (browsingControls)
                controls.itemAt(controlIndex).clicked();
            else
                command("pause");
        }
    }
    function handleKey(key) {
        let directions = {};
        directions[Qt.Key_Left] = "left";
        directions[Qt.Key_Right] = "right";
        directions[Qt.Key_Up] = "up";
        directions[Qt.Key_Down] = "down";
        directions[Qt.Key_Escape] = "back";
        directions[Qt.Key_Return] = "press";
        directions[Qt.Key_Enter] = "press";
        if (directions[key])
            navigate(directions[key]);
        else if (key === Qt.Key_Space)
            command("pause");
        else if (key === Qt.Key_X)
            openMenu("Subtitles");
        else if (key === Qt.Key_Y)
            openMenu("Audio");
        else if (key === Qt.Key_Q)
            openMenu("Quality");
        else if (key === Qt.Key_N)
            command("skip");
        else if (key === Qt.Key_Plus || key === Qt.Key_Equal)
            command("volume", 5);
        else if (key === Qt.Key_Minus)
            command("volume", -5);
        else if (key === Qt.Key_Home)
            action({
                type: "home"
            });
        else
            return false;
        return true;
    }
    onVisibleChanged: {
        if (visible) {
            browsingControls = false;
            menu = "";
            forceActiveFocus();
            activity();
        }
    }
    onPausedChanged: if (paused)
        activity()
    onNoticeChanged: if (notice)
        activity()
    Keys.onPressed: event => {
        event.accepted = handleKey(event.key);
    }
    Timer {
        id: hideTimer
        interval: 4500
        onTriggered: if (!player.paused && !player.buffering && !player.menu && !player.browsingControls)
            player.controlsVisible = false
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onPositionChanged: player.activity()
        onClicked: {
            player.browsingControls = false;
            player.forceActiveFocus();
            player.command("pause");
        }
    }
    Item {
        width: 1920
        height: 1080
        anchors.centerIn: parent
        scale: Math.min(player.width / width, player.height / height)
        Item {
            anchors.fill: parent
            visible: player.controlsVisible || player.paused || player.buffering || !!player.menu
            Rectangle {
                width: parent.width
                height: 280
                gradient: Gradient {
                    GradientStop {
                        position: 0
                        color: "#d9000000"
                    }
                    GradientStop {
                        position: 1
                        color: "#00000000"
                    }
                }
            }
            Text {
                x: 100
                y: 75
                width: 1340
                text: player.title
                elide: Text.ElideRight
                font.family: "Inter"
                font.pixelSize: 36
                font.weight: Font.Medium
                color: "white"
            }
            TvButton {
                x: 1640
                y: 65
                width: 180
                height: 64
                text: "Home"
                iconName: "home"
                reducedMotion: player.reducedMotion
                onClicked: player.action({
                    type: "home"
                })
            }
            Rectangle {
                y: 690
                width: parent.width
                height: 390
                gradient: Gradient {
                    GradientStop {
                        position: 0
                        color: "#00000000"
                    }
                    GradientStop {
                        position: 0.55
                        color: "#d9000000"
                    }
                    GradientStop {
                        position: 1
                        color: "#ed000000"
                    }
                }
            }
            Text {
                x: 100
                y: 775
                width: 1500
                text: player.notice || (player.buffering ? "Buffering…" : player.paused ? "Paused" : "")
                font.family: "Inter"
                font.pixelSize: 24
                color: "#e5e7eb"
                elide: Text.ElideRight
            }
            Slider {
                id: timeline
                objectName: "playerTimeline"
                x: 92
                y: 820
                width: 1736
                height: 40
                from: 0
                to: Math.max(1, player.playback.duration || 0)
                Binding {
                    target: timeline
                    property: "value"
                    value: player.playback.position || 0
                    when: !timeline.pressed
                    restoreMode: Binding.RestoreNone
                }
                enabled: (player.playback.duration || 0) > 0
                onMoved: player.activity()
                onPressedChanged: if (!pressed)
                    player.command("seekTo", value)
                background: Rectangle {
                    x: timeline.leftPadding
                    y: timeline.topPadding + timeline.availableHeight / 2 - height / 2
                    width: timeline.availableWidth
                    height: 5
                    radius: 2
                    color: "#667078"
                    Rectangle {
                        width: timeline.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: player.accent
                    }
                }
                handle: Rectangle {
                    x: timeline.leftPadding + timeline.visualPosition * (timeline.availableWidth - width)
                    y: timeline.topPadding + timeline.availableHeight / 2 - height / 2
                    width: 16
                    height: 16
                    radius: 8
                    color: "white"
                }
            }
            Text {
                x: 100
                y: 865
                text: player.time(player.playback.position) + " / " + player.time(player.playback.duration)
                font.family: "Inter"
                font.pixelSize: 24
                color: "#e5e7eb"
            }
            Text {
                x: 1450
                y: 865
                width: 370
                horizontalAlignment: Text.AlignRight
                text: "Volume " + Math.round(player.playback.volume === undefined ? 100 : player.playback.volume) + "%"
                font.family: "Inter"
                font.pixelSize: 24
                color: "#e5e7eb"
            }
            Row {
                x: 100
                y: 925
                spacing: 16
                Repeater {
                    id: controls
                    model: ["pause", "Audio", "Subtitles", "Quality", "quieter", "louder", "skip", "stop"]
                    TvButton {
                        required property string modelData
                        required property int index
                        objectName: "player_" + modelData
                        width: 201
                        height: 64
                        text: modelData === "pause" ? (player.paused ? "Play" : "Pause") : modelData === "quieter" ? "Volume −" : modelData === "louder" ? "Volume +" : modelData === "skip" ? "Skip" : modelData === "stop" ? "Back" : modelData
                        selected: player.browsingControls && player.controlIndex === index
                        accent: player.accent
                        reducedMotion: player.reducedMotion
                        onClicked: {
                            if (["Audio", "Subtitles", "Quality"].indexOf(modelData) >= 0)
                                player.openMenu(modelData);
                            else
                                player.command(modelData === "quieter" || modelData === "louder" ? "volume" : modelData, modelData === "quieter" ? -5 : 5);
                        }
                    }
                }
            }
            Text {
                x: 100
                y: 1022
                text: "← → Seek    ↓ Controls    A / Space Pause    B / Esc Back    X Subtitles    Y Audio"
                font.family: "Inter"
                font.pixelSize: 20
                color: "#a7afb9"
            }
        }
        BusyIndicator {
            anchors.centerIn: parent
            running: player.buffering
            visible: running
            width: 72
            height: 72
        }
        Rectangle {
            anchors.fill: parent
            color: "#90000000"
            visible: !!player.menu
            MouseArea {
                anchors.fill: parent
                onClicked: {
                    player.menu = "";
                    player.focusControl();
                }
            }
            Rectangle {
                x: 1120
                y: 175
                width: 700
                height: 680
                radius: 14
                color: "#1c2129"
                Text {
                    x: 36
                    y: 30
                    text: player.menu
                    font.family: "Inter"
                    font.pixelSize: 34
                    font.weight: Font.Medium
                    color: "white"
                }
                Text {
                    x: 36
                    y: 125
                    visible: !player.menuEntries.length
                    text: "No tracks available"
                    font.pixelSize: 26
                    color: "#a7afb9"
                }
                ListView {
                    id: menuList
                    objectName: "playerMenu"
                    x: 30
                    y: 110
                    width: 640
                    height: 470
                    spacing: 12
                    clip: true
                    model: player.menuEntries
                    delegate: TvButton {
                        required property var modelData
                        required property int index
                        width: 620
                        height: 64
                        text: (modelData.selected ? "✓  " : "") + modelData.label
                        selected: player.menuIndex === index
                        primary: player.menuIndex === index
                        accent: player.accent
                        reducedMotion: player.reducedMotion
                        onClicked: player.selectMenu(index)
                    }
                }
                TvButton {
                    x: 36
                    y: 600
                    text: "Close"
                    height: 54
                    onClicked: {
                        player.menu = "";
                        player.focusControl();
                    }
                }
            }
        }
    }
}
