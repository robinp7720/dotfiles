import QtQuick
import QtQuick.Controls

Item {
    id: control
    property string text: ""
    property string icon: ""
    property string iconName: ""
    property bool quiet: false
    property bool primary: false
    property bool compact: false
    property bool hold: false
    property bool selected: false
    property color accent: "#83d2e3"
    property bool reducedMotion: false
    property real progress: 0
    signal clicked
    signal pressed
    signal released
    implicitWidth: Math.max(compact ? 64 : 120, label.implicitWidth + (iconName && text ? 42 : 0) + (compact ? 32 : 48))
    implicitHeight: compact ? 64 : 68
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: clicked()
    scale: activeFocus ? 1.025 : 1
    Behavior on scale {
        NumberAnimation {
            duration: control.reducedMotion ? 0 : 160
            easing.type: Easing.OutCubic
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: 10
        color: control.activeFocus ? "#f2f3f5" : control.primary ? control.accent : control.selected ? "#30363e" : control.quiet ? "transparent" : "#242930"
        opacity: control.enabled ? 1 : 0.4
        border.width: control.activeFocus ? 2 : 0
        border.color: "#ffffff"
        Behavior on color {
            ColorAnimation {
                duration: control.reducedMotion ? 0 : 160
            }
        }
        Rectangle {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            height: 5
            radius: 2
            width: parent.width * control.progress
            color: control.accent
            visible: control.progress > 0
        }
    }
    Icon {
        x: control.text ? 22 : (control.width - width) / 2
        anchors.verticalCenter: parent.verticalCenter
        width: 28
        height: 28
        name: control.iconName
        visible: !!name
        color: control.activeFocus || control.primary ? "#14171c" : control.selected ? control.accent : "#a7afb9"
    }
    Text {
        id: label
        width: control.width - (control.compact ? 20 : 48) - (control.iconName && control.text ? 42 : 0)
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        anchors.centerIn: parent
        anchors.horizontalCenterOffset: control.iconName && control.text ? 21 : 0
        text: control.icon + (control.icon && control.text ? "  " : "") + control.text
        color: control.activeFocus || control.primary ? "#14171c" : "#e5e7eb"
        font.family: "Inter"
        font.pixelSize: control.compact ? 29 : 24
        font.weight: Font.DemiBold
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onEntered: control.forceActiveFocus()
        onPressed: {
            control.forceActiveFocus();
            control.pressed();
        }
        onReleased: control.released()
        onCanceled: control.released()
        onClicked: if (!control.hold)
            control.clicked()
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            if (!event.isAutoRepeat) {
                control.pressed();
                if (!control.hold)
                    control.clicked();
            }
            event.accepted = true;
        }
    }
    Keys.onReleased: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            control.released();
            event.accepted = true;
        }
    }
    onActiveFocusChanged: if (!activeFocus)
        released()
}
