import QtQuick

Item {
    id: ambient
    property color accent: "#83d2e3"
    property real drift: 0
    property bool reducedMotion: false
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: "#0a1019"
            }
            GradientStop {
                position: 0.6
                color: "#122732"
            }
            GradientStop {
                position: 1
                color: "#26404a"
            }
        }
    }
    Canvas {
        id: sky
        anchors.fill: parent
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            let c = getContext("2d"), w = width, h = height;
            c.reset();
            let g = c.createRadialGradient(w * .78, h * .38, 0, w * .78, h * .38, w * .55);
            g.addColorStop(0, "#46717b");
            g.addColorStop(.45, "#233d50");
            g.addColorStop(1, "#0b111b");
            c.fillStyle = g;
            c.fillRect(0, 0, w, h);
            c.save();
            c.translate(w * .78, h * .42);
            c.rotate(-.42);
            for (let i = 0; i < 14; i++) {
                c.beginPath();
                c.ellipse(-w * .28 - i * 11, -h * .24 - i * 7, w * .56 + i * 22, h * .48 + i * 14);
                c.strokeStyle = "rgba(150,204,215," + (0.13 - i * .007) + ")";
                c.lineWidth = 1.3;
                c.stroke();
            }
            c.restore();
        }
    }
    Item {
        x: parent.width * .65 + ambient.drift * 22
        y: parent.height * .19
        width: 420
        height: 420
        Behavior on x {
            NumberAnimation {
                duration: ambient.reducedMotion ? 0 : 600
                easing.type: Easing.OutCubic
            }
        }
        Rectangle {
            anchors.centerIn: parent
            width: 600
            height: 600
            radius: 300
            color: "#087fd8e3"
        }
        Rectangle {
            anchors.centerIn: parent
            width: 490
            height: 490
            radius: 245
            color: "#0a9ddeee"
        }
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: "#09202c"
                }
                GradientStop {
                    position: .7
                    color: "#183b4b"
                }
                GradientStop {
                    position: 1
                    color: "#5b8e9b"
                }
            }
            border.width: 1
            border.color: "#416b7a"
        }
        Rectangle {
            x: 24
            y: 24
            width: parent.width - 48
            height: parent.height - 48
            radius: width / 2
            color: "transparent"
            border.width: 1
            border.color: "#1c8da9b7"
        }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: "#f00a1018"
            }
            GradientStop {
                position: .55
                color: "#600a1018"
            }
            GradientStop {
                position: 1
                color: "#000a1018"
            }
        }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop {
                position: .35
                color: "#000a1018"
            }
            GradientStop {
                position: 1
                color: "#ff0a1018"
            }
        }
    }
}
