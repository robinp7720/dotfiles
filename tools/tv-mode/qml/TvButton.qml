import QtQuick
import QtQuick.Controls

Item {
    id: control
    property string text: ""
    property string icon: ""
    property string iconName: ""
    property bool quiet: false
    property bool flat: false
    property string valueText: ""
    property bool primary: false
    property bool compact: false
    property bool hold: false
    property bool selected: false
    property color accent: "#83d2e3"
    property bool reducedMotion: false
    property real progress: 0
    property real labelOpacity: 1
    property real labelAreaWidth: width
    property real surfaceWidth: width
    readonly property bool lightSurface: (surface.color.r * 0.213 + surface.color.g * 0.715 + surface.color.b * 0.072) * surface.color.a > 0.5
    signal clicked
    signal pressed
    signal released
    implicitWidth: Math.max(compact ? 64 : 120, label.implicitWidth + (iconName && text ? 42 : 0) + (compact ? 32 : 48))
    implicitHeight: compact ? 64 : 68
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: clicked()
    Rectangle {
        id: surface
        width: control.surfaceWidth
        height: parent.height
        radius: 3
        color: control.activeFocus ? "white" : control.primary ? "white" : control.selected && !control.quiet ? "#444444" : control.quiet || control.flat ? "transparent" : "#333333"
        opacity: control.enabled ? 1 : 0.4
        border.width: control.activeFocus ? 2 : control.primary ? 1 : 0
        border.color: "#ffffff"
        Behavior on color {
            ColorAnimation {
                duration: control.reducedMotion ? 0 : 160
                onDurationChanged: if (duration === 0)
                    complete()
            }
        }
        Rectangle {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            height: 5
            radius: 2
            width: parent.width * control.progress
            color: "#e50914"
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
        color: control.lightSurface ? "#14171c" : control.selected ? "white" : "#b3b3b3"
    }
    Text {
        id: label
        opacity: control.labelOpacity
        width: control.labelAreaWidth - (control.compact ? 20 : 48) - (control.iconName && control.text ? 42 : 0) - (control.valueText ? 350 : 0)
        horizontalAlignment: control.flat ? Text.AlignLeft : Text.AlignHCenter
        elide: Text.ElideRight
        anchors.centerIn: parent
        anchors.horizontalCenterOffset: (control.labelAreaWidth - control.width) / 2 + (control.iconName && control.text ? 21 : 0) - (control.valueText ? 175 : 0)
        text: control.icon + (control.icon && control.text ? "  " : "") + control.text
        color: control.lightSurface ? "#141414" : "#eeeeee"
        font.family: "Inter"
        font.pixelSize: control.compact ? 29 : 24
        font.weight: Font.DemiBold
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: 16
        anchors.rightMargin: 16
        height: 3
        color: "white"
        visible: control.selected && control.quiet && !control.activeFocus
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 1
        color: "#333333"
        visible: control.flat && !control.activeFocus
    }
    Text {
        anchors.right: parent.right
        anchors.rightMargin: 24
        anchors.verticalCenter: parent.verticalCenter
        width: 320
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        text: control.valueText
        visible: !!text
        font.family: "Inter"
        font.pixelSize: 22
        color: control.lightSurface ? "#444444" : "#b3b3b3"
    }
    Rectangle {
        anchors.fill: parent
        anchors.margins: -5
        radius: 5
        color: "transparent"
        border.width: 2
        border.color: "white"
        visible: control.primary && control.activeFocus
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
