import QtQuick
import QtQuick.Controls

Item {
    id: control
    property string text: ""
    property string icon: ""
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
    implicitWidth: Math.max(compact ? 64 : 120, label.implicitWidth + (compact ? 32 : 52))
    implicitHeight: compact ? 64 : 68
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: clicked()
    scale: activeFocus ? 1.045 : 1
    Behavior on scale {
        NumberAnimation {
            duration: control.reducedMotion ? 0 : 160
            easing.type: Easing.OutCubic
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: control.compact ? 22 : 18
        color: control.activeFocus ? (control.primary ? "#ffffff" : "#eaf3f5") : control.primary ? control.accent : control.selected ? "#35484b" : "#283039"
        opacity: control.enabled ? 1 : 0.4
        border.width: control.activeFocus ? 3 : 1
        border.color: control.activeFocus ? "#ffffff" : "#30495760"
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
    Text {
        id: label
        width: control.width - (control.compact ? 20 : 48)
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        anchors.centerIn: parent
        text: control.icon + (control.icon && control.text ? "  " : "") + control.text
        color: control.activeFocus || control.primary ? "#102025" : "#e5ebee"
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
