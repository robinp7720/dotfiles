import QtQuick

Item {
    id: card
    property var media: ({})
    property var assets: ({})
    property color accent: "#83d2e3"
    property bool reducedMotion: false
    property real artHeight: wide ? 214 : 306
    property bool compact: false
    property bool wide: media.kind === "episode"
    signal chosen(var media)
    signal highlighted(var media)
    width: wide ? 380 : 218
    height: wide ? 310 : 380
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: media.title || "Title"
    Accessible.onPressAction: chosen(media)
    z: activeFocus ? 2 : 0
    scale: activeFocus ? 1.025 : 1
    Behavior on scale {
        NumberAnimation {
            duration: card.reducedMotion ? 0 : 180
            onDurationChanged: if (duration === 0)
                complete()
            easing.type: Easing.OutCubic
        }
    }
    Rectangle {
        id: frame
        width: parent.width
        height: card.artHeight
        radius: 3
        color: "#232323"
        Rectangle {
            anchors.fill: parent
            radius: 3
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: "#303030"
                }
                GradientStop {
                    position: 1
                    color: "#181818"
                }
            }
        }
        Text {
            anchors.centerIn: parent
            width: parent.width - 40
            text: card.media.title || ""
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: 25
            font.family: "Inter"
            color: "#b3b3b3"
        }
        Image {
            anchors.fill: parent
            anchors.margins: 0
            source: card.assets[card.media.art] || ""
            asynchronous: true
            fillMode: Image.PreserveAspectCrop
            sourceSize.width: card.wide ? 760 : 436
            sourceSize.height: card.wide ? 428 : 612
            opacity: status === Image.Ready ? 1 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: card.reducedMotion ? 0 : 260
                    onDurationChanged: if (duration === 0)
                        complete()
                }
            }
            clip: true
        }
        Rectangle {
            anchors.fill: parent
            radius: 3
            color: "transparent"
            border.width: card.activeFocus ? 4 : 0
            border.color: "#f2f7fa"
        }
        Rectangle {
            x: 14
            y: parent.height - 17
            width: parent.width - 28
            height: 5
            radius: 3
            color: "#90131d22"
            visible: card.media.progress > 0
            Rectangle {
                width: parent.width * Math.min(1, card.media.progress || 0)
                height: 5
                radius: 3
                color: "#e50914"
            }
        }
        Rectangle {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 12
            width: 34
            height: 34
            radius: 3
            color: "#e50914"
            visible: card.media.progress >= 0.9
            Text {
                anchors.centerIn: parent
                text: "✓"
                font.pixelSize: 21
                color: "white"
            }
        }
    }
    Text {
        y: frame.height + (card.compact ? 11 : 17)
        width: parent.width
        text: card.media.title || ""
        font.family: "Inter"
        font.pixelSize: card.compact ? 21 : 23
        font.weight: card.activeFocus ? Font.DemiBold : Font.Medium
        color: card.activeFocus ? "#ffffff" : "#dddddd"
        elide: Text.ElideRight
    }
    Text {
        y: frame.height + (card.compact ? 39 : 48)
        width: parent.width
        text: card.media.subtitle || ""
        font.family: "Inter"
        font.pixelSize: card.compact ? 17 : 18
        color: "#999999"
        elide: Text.ElideRight
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onEntered: card.forceActiveFocus()
        onClicked: card.chosen(card.media)
    }
    Keys.onReturnPressed: chosen(media)
    Keys.onEnterPressed: chosen(media)
    Keys.onSpacePressed: chosen(media)
    onActiveFocusChanged: if (activeFocus)
        highlighted(media)
}
