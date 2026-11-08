import QtQuick

// Playback has its own quiet controls; library tiles keep their existing style.
Item {
    id: control
    property string text: ""
    property string label: text
    property string iconName: ""
    property bool outlined: false
    property bool flat: false
    property bool selected: false
    property bool reducedMotion: false
    property bool tooltipBelow: false
    readonly property bool highlighted: activeFocus || pointer.containsMouse || selected
    readonly property color foreground: text && !flat && highlighted ? "#161616" : "white"
    signal clicked
    implicitWidth: 80
    implicitHeight: 80
    activeFocusOnTab: true
    opacity: enabled ? 1 : 0.3
    Accessible.role: Accessible.Button
    Accessible.name: label
    Accessible.onPressAction: if (enabled) clicked()
    Rectangle {
        anchors.fill: parent
        radius: control.text ? 3 : height / 2
        color: control.highlighted ? (control.text && !control.flat ? "white" : "#28ffffff") : control.outlined ? "#a6000000" : "transparent"
        border.width: control.outlined || (control.activeFocus && !control.flat) ? 2 : 0
        border.color: "white"
        Behavior on color { ColorAnimation { duration: control.reducedMotion ? 0 : 120 } }
    }
    Icon {
        anchors.verticalCenter: parent.verticalCenter
        x: control.text ? 20 : (parent.width - width) / 2
        width: control.text ? 28 : 38
        height: width
        name: control.iconName
        color: control.foreground
        visible: !!name
    }
    Text {
        anchors.verticalCenter: parent.verticalCenter
        x: control.flat || control.iconName ? 64 : 20
        width: parent.width - x - 20
        text: control.text
        visible: !!text
        horizontalAlignment: control.flat ? Text.AlignLeft : Text.AlignHCenter
        elide: Text.ElideRight
        font.family: "Inter"
        font.pixelSize: 25
        font.weight: control.flat ? Font.Normal : Font.DemiBold
        color: control.foreground
    }
    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        y: control.tooltipBelow ? control.height + 12 : -height - 16
        width: caption.implicitWidth + 28
        height: 44
        radius: 3
        color: "#ed181818"
        visible: !control.text && control.highlighted && control.enabled
        Text {
            id: caption
            anchors.centerIn: parent
            text: control.label
            font.family: "Inter"
            font.pixelSize: 22
            color: "white"
        }
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onPressed: control.forceActiveFocus()
        onClicked: control.clicked()
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            if (!event.isAutoRepeat && enabled) clicked();
            event.accepted = true;
        }
    }
}
