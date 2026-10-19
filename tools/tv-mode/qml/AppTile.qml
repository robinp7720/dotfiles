import QtQuick

Item {
    id: tile
    property string title: ""
    property string iconName: ""
    property url artwork: ""
    property color tint: "#202a30"
    property color accent: "#83d2e3"
    property bool reducedMotion: false
    property bool hold: false
    signal clicked
    width: 536
    height: 224
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: title
    Accessible.onPressAction: clicked()
    scale: activeFocus ? 1.018 : 1
    z: activeFocus ? 2 : 0
    Behavior on scale {
        NumberAnimation {
            duration: tile.reducedMotion ? 0 : 160
            easing.type: Easing.OutCubic
        }
    }
    Rectangle {
        anchors.fill: parent
        radius: 14
        color: tile.tint
        border.width: tile.activeFocus ? 3 : 1
        border.color: tile.activeFocus ? "#f2f3f5" : "#16ffffff"
    }
    Icon {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 36
        width: 88
        height: 88
        name: tile.iconName
        color: tile.accent
        visible: logo.status !== Image.Ready
    }
    Image {
        id: logo
        anchors.horizontalCenter: parent.horizontalCenter
        y: 36
        width: 88
        height: 88
        source: tile.artwork
        sourceSize.width: 176
        sourceSize.height: 176
        fillMode: Image.PreserveAspectFit
        visible: status === Image.Ready
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 151
        text: tile.title
        color: "#f2f3f5"
        font.family: "Inter"
        font.pixelSize: 30
        font.weight: Font.Medium
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onEntered: tile.forceActiveFocus()
        onClicked: tile.clicked()
    }
    Keys.onReturnPressed: clicked()
    Keys.onEnterPressed: clicked()
    Keys.onSpacePressed: clicked()
}
