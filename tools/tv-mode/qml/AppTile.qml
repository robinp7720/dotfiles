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
    width: 360
    height: 104
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: title
    Accessible.onPressAction: clicked()
    Rectangle {
        anchors.fill: parent
        radius: 4
        color: tile.activeFocus ? "#f5f5f5" : "#c0202020"
        border.width: tile.activeFocus ? 2 : 1
        border.color: tile.activeFocus ? "#f2f3f5" : "#16ffffff"
    }
    Icon {
        x: 28
        anchors.verticalCenter: parent.verticalCenter
        width: 56
        height: 56
        name: tile.iconName
        color: tile.activeFocus ? "#141414" : "white"
        visible: logo.status !== Image.Ready
    }
    Image {
        id: logo
        x: 28
        anchors.verticalCenter: parent.verticalCenter
        width: 56
        height: 56
        source: tile.artwork
        sourceSize.width: 112
        sourceSize.height: 112
        fillMode: Image.PreserveAspectFit
        visible: status === Image.Ready
    }
    Text {
        x: 112
        anchors.verticalCenter: parent.verticalCenter
        text: tile.title
        color: tile.activeFocus ? "#141414" : "#f2f3f5"
        font.family: "Inter"
        font.pixelSize: 28
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
