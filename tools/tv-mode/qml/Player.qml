import QtQuick
import QtQuick.Controls

Item {
    id: player
    property var playback: ({})
    property string title: ""
    property string subtitle: ""
    property var nextEpisode: ({})
    property bool switchingEpisode: false
    property string notice: ""
    property color accent: "#83d2e3"
    property bool reducedMotion: false
    property bool controlsVisible: true
    property bool browsingControls: false
    property bool browsingActions: false
    property int actionIndex: 0
    readonly property string actionKey: (playback.skipLabel ? "skip," : "") + (nextEpisode.available ? "nextEpisode," : "")
    readonly property var availableActions: actionKey.split(",").filter(value => !!value)
    property int controlIndex: 0
    property string menu: ""
    property var menuEntries: []
    property int menuIndex: 0
    readonly property bool paused: !!playback.paused
    readonly property bool buffering: !!playback.buffering
    signal action(var value)
    function command(name, value) {
        if (switchingEpisode && name !== "stop")
            return;
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
        if (browsingActions && contextActions.itemAt(actionIndex))
            contextActions.itemAt(actionIndex).forceActiveFocus();
        else if (browsingControls && controls.itemAt(controlIndex))
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
            browsingActions = false;
            browsingControls = true;
            focusControl();
            return;
        }
        if (key === "up") {
            browsingActions = !browsingControls && availableActions.length > 0;
            browsingControls = false;
            focusControl();
            return;
        }
        if (key === "left" || key === "right") {
            if (browsingActions) {
                actionIndex = Math.max(0, Math.min(availableActions.length - 1, actionIndex + (key === "right" ? 1 : -1)));
                focusControl();
            } else if (browsingControls) {
                controlIndex = Math.max(0, Math.min(controls.count - 1, controlIndex + (key === "right" ? 1 : -1)));
                if (!controls.itemAt(controlIndex).enabled)
                    controlIndex = Math.max(0, Math.min(controls.count - 1, controlIndex + (key === "right" ? 1 : -1)));
                focusControl();
            } else
                command("seek", key === "right" ? 10 : -10);
        } else if (key === "press") {
            if (browsingActions && contextActions.itemAt(actionIndex))
                contextActions.itemAt(actionIndex).clicked();
            else if (browsingControls) {
                if (controls.itemAt(controlIndex).enabled)
                    controls.itemAt(controlIndex).clicked();
            }
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
        else if (key === Qt.Key_E && nextEpisode.available)
            command("nextEpisode");
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
            browsingActions = false;
            controlIndex = 0;
            actionIndex = 0;
            menu = "";
            forceActiveFocus();
            activity();
        }
    }
    onPausedChanged: activity()
    onBufferingChanged: activity()
    onAvailableActionsChanged: {
        // A disappearing intro must never turn a pending A press into Next episode.
        if (browsingActions) {
            browsingActions = false;
            actionIndex = 0;
            forceActiveFocus();
        }
    }
    onNoticeChanged: if (notice)
        activity()
    Keys.onPressed: event => {
        event.accepted = handleKey(event.key);
    }
    Timer {
        id: hideTimer
        interval: 4500
        onTriggered: if (!player.paused && !player.buffering && !player.menu && !player.browsingControls && !player.browsingActions)
            player.controlsVisible = false
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onPositionChanged: player.activity()
        onClicked: {
            player.browsingControls = false;
            player.browsingActions = false;
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
            opacity: player.controlsVisible || player.paused || player.buffering || !!player.menu ? 1 : 0
            visible: opacity > 0
            Behavior on opacity { NumberAnimation { duration: player.reducedMotion ? 0 : 180 } }
            Rectangle {
                width: parent.width
                height: 260
                gradient: Gradient {
                    GradientStop { position: 0; color: "#c9000000" }
                    GradientStop { position: 1; color: "transparent" }
                }
            }
            PlayerControl {
                x: 56; y: 58
                iconName: "back"
                label: "Back to details"
                tooltipBelow: true
                reducedMotion: player.reducedMotion
                onClicked: player.command("stop")
            }
            Text {
                x: 152; y: 70; width: 1450
                text: player.title
                elide: Text.ElideRight
                font.family: "Inter"; font.pixelSize: 34; font.weight: Font.DemiBold
                color: "white"
            }
            PlayerControl {
                x: 1784; y: 58
                iconName: "home"
                label: "Home"
                tooltipBelow: true
                reducedMotion: player.reducedMotion
                onClicked: player.action({type: "home"})
            }
            Rectangle {
                y: 650; width: parent.width; height: 430
                gradient: Gradient {
                    GradientStop { position: 0; color: "transparent" }
                    GradientStop { position: 0.55; color: "#a6000000" }
                    GradientStop { position: 1; color: "#ed000000" }
                }
            }
            Text {
                x: 64; y: 847; width: 1500
                text: player.notice || (player.buffering ? "Loading…" : "")
                font.family: "Inter"; font.pixelSize: 24
                color: "#eeeeee"
                elide: Text.ElideRight
            }
            Slider {
                id: timeline
                objectName: "playerTimeline"
                x: 56; y: 896; width: 1656; height: 44
                from: 0
                to: Math.max(1, player.playback.duration || 0)
                Binding {
                    target: timeline
                    property: "value"
                    value: player.playback.position || 0
                    when: !timeline.pressed
                    restoreMode: Binding.RestoreNone
                }
                enabled: (player.playback.duration || 0) > 0 && !player.switchingEpisode
                onMoved: player.activity()
                onPressedChanged: if (!pressed) player.command("seekTo", value)
                background: Rectangle {
                    x: timeline.leftPadding
                    y: timeline.topPadding + timeline.availableHeight / 2 - height / 2
                    width: timeline.availableWidth; height: timeline.hovered || timeline.pressed ? 7 : 5
                    color: "#70ffffff"
                    Rectangle {
                        width: timeline.visualPosition * parent.width; height: parent.height
                        color: "#e50914"
                    }
                }
                handle: Rectangle {
                    x: timeline.leftPadding + timeline.visualPosition * (timeline.availableWidth - width)
                    y: timeline.topPadding + timeline.availableHeight / 2 - height / 2
                    width: 18; height: 18; radius: 9
                    color: "#e50914"
                    visible: timeline.hovered || timeline.pressed || player.browsingControls || player.paused
                }
                ToolTip.visible: pressed
                ToolTip.text: player.time(value)
            }
            Text {
                x: 1736; y: 902; width: 120
                horizontalAlignment: Text.AlignRight
                text: player.playback.duration > 0 ? "−" + player.time(Math.max(0, player.playback.duration - player.playback.position)) : "--:--"
                font.family: "Inter"; font.pixelSize: 25
                color: "white"
                Accessible.name: text + " remaining"
            }
            Text {
                x: 552; y: 973; width: 780
                text: player.subtitle || player.title
                elide: Text.ElideRight
                font.family: "Inter"; font.pixelSize: 26; font.weight: Font.Medium
                color: "#eeeeee"
            }
            Repeater {
                id: controls
                model: ["pause", "rewind", "forward", "quieter", "louder", "chapter", "Audio", "Subtitles", "Quality", "stop"]
                PlayerControl {
                    required property string modelData
                    required property int index
                    objectName: "player_" + modelData
                    x: index < 5 ? 56 + index * 88 : 1392 + (index - 5) * 88
                    y: 950
                    iconName: modelData === "pause" ? (player.paused ? "play" : "pause") : modelData === "quieter" ? "volumeDown" : modelData === "louder" ? "volumeUp" : modelData === "Audio" ? "audio" : modelData === "Subtitles" ? "subtitles" : modelData === "Quality" ? "quality" : modelData === "stop" ? "back" : modelData
                    label: modelData === "pause" ? (player.paused ? "Play" : "Pause") : modelData === "rewind" ? "Back 10 seconds" : modelData === "forward" ? "Forward 10 seconds" : modelData === "quieter" || modelData === "louder" ? "Volume " + Math.round(player.playback.volume === undefined ? 100 : player.playback.volume) + "%" : modelData === "chapter" ? "Next chapter" : modelData === "stop" ? "Back to details" : modelData
                    enabled: (!player.switchingEpisode || modelData === "stop") && (modelData !== "chapter" || !!player.playback.canSkipChapter)
                    reducedMotion: player.reducedMotion
                    onActiveFocusChanged: if (activeFocus) {
                        player.browsingControls = true;
                        player.browsingActions = false;
                        player.controlIndex = index;
                        player.activity();
                    }
                    onClicked: {
                        if (["Audio", "Subtitles", "Quality"].indexOf(modelData) >= 0)
                            player.openMenu(modelData);
                        else if (modelData === "rewind" || modelData === "forward")
                            player.command("seek", modelData === "rewind" ? -10 : 10);
                        else
                            player.command(modelData === "quieter" || modelData === "louder" ? "volume" : modelData, modelData === "quieter" ? -5 : 5);
                    }
                }
            }
        }
        Column {
            anchors.right: parent.right; anchors.rightMargin: 64
            y: 734; spacing: 18
            visible: !player.menu && (player.controlsVisible || !!player.playback.skipLabel || player.browsingActions)
            Text {
                width: 740
                horizontalAlignment: Text.AlignRight
                elide: Text.ElideRight
                text: player.nextEpisode.available ? "Up next: " + player.nextEpisode.label + " · " + player.nextEpisode.title : player.nextEpisode.loading ? "Finding next episode…" : player.nextEpisode.error ? "Next episode unavailable" : ""
                font.family: "Inter"; font.pixelSize: 24
                color: "white"
                style: Text.Outline; styleColor: "#80000000"
                visible: player.controlsVisible && !!text
            }
            Row {
                anchors.right: parent.right; spacing: 16
                Repeater {
                    id: contextActions
                    model: player.availableActions
                    PlayerControl {
                        required property string modelData
                        required property int index
                        objectName: "player_" + modelData
                        width: modelData === "skip" ? 232 : 280
                        height: 64
                        text: modelData === "skip" ? (player.playback.skipLabel || "Skip intro") : player.switchingEpisode ? "Opening…" : "Next episode"
                        iconName: modelData === "nextEpisode" ? "next" : ""
                        outlined: true
                        enabled: !player.switchingEpisode
                        reducedMotion: player.reducedMotion
                        onActiveFocusChanged: if (activeFocus) {
                            player.browsingActions = true;
                            player.browsingControls = false;
                            player.actionIndex = index;
                            player.activity();
                        }
                        onClicked: if (enabled) player.command(modelData)
                    }
                }
            }
        }
        BusyIndicator {
            anchors.centerIn: parent
            running: player.buffering
            visible: running
            width: 72; height: 72
        }
        Rectangle {
            anchors.fill: parent
            color: "#66000000"
            visible: !!player.menu
            MouseArea {
                anchors.fill: parent
                onClicked: {
                    player.menu = "";
                    player.focusControl();
                }
            }
            Rectangle {
                x: 1224; y: 316; width: 632; height: 560
                radius: 4
                color: "#f0181818"
                Text {
                    x: 32; y: 28
                    text: player.menu
                    font.family: "Inter"; font.pixelSize: 30; font.weight: Font.DemiBold
                    color: "white"
                }
                PlayerControl {
                    x: 528; y: 12
                    iconName: "close"
                    label: "Close"
                    tooltipBelow: true
                    onClicked: {
                        player.menu = "";
                        player.focusControl();
                    }
                }
                Text {
                    x: 32; y: 120
                    visible: !player.menuEntries.length
                    text: "No tracks available"
                    font.pixelSize: 25
                    color: "#b3b3b3"
                }
                ListView {
                    id: menuList
                    objectName: "playerMenu"
                    x: 16; y: 104; width: 600; height: 430
                    spacing: 4
                    clip: true
                    model: player.menuEntries
                    delegate: PlayerControl {
                        required property var modelData
                        required property int index
                        width: 600; height: 64
                        text: modelData.label
                        iconName: modelData.selected ? "check" : ""
                        flat: true
                        selected: player.menuIndex === index
                        reducedMotion: player.reducedMotion
                        onClicked: player.selectMenu(index)
                    }
                }
            }
        }
    }
}
