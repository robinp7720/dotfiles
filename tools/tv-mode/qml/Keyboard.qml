import QtQuick

Rectangle {
    id: keyboard
    property var target: null
    property bool shifted: false
    property bool symbols: false
    property color accent: "#83d2e3"
    property bool reducedMotion: false
    signal closed
    width: 1080
    height: 450
    radius: 4
    color: "#fc181818"
    border.color: "#454545"
    border.width: 1
    function open(field) {
        target = field;
        visible = true;
    }
    function finish() {
        let previous = target;
        visible = false;
        target = null;
        if (previous)
            previous.forceActiveFocus();
        closed();
    }
    function enter(text) {
        if (target) {
            target.insert(target.cursorPosition, text);
        }
    }
    Text {
        objectName: "keyboardPreview"
        x: 50
        y: 24
        width: 980
        text: !keyboard.target ? "" : keyboard.target.echoMode === TextInput.Password ? "•".repeat(keyboard.target.text.length) : keyboard.target.text || keyboard.target.placeholderText || "Enter text"
        elide: Text.ElideLeft
        color: keyboard.target && keyboard.target.text ? "white" : "#999999"
        font.family: "Inter"
        font.pixelSize: 28
    }
    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 32
        spacing: 12
        Repeater {
            model: keyboard.symbols ? ["1234567890", "@#$%&*()-_", "+=!?/.:,;'"] : ["qwertyuiop", "asdfghjkl'", "zxcvbnm.-_"]
            Row {
                required property string modelData
                spacing: 10
                Repeater {
                    id: firstKey
                    model: parent.modelData.split("")
                    TvButton {
                        required property string modelData
                        width: 88
                        height: 68
                        text: keyboard.shifted ? modelData.toUpperCase() : modelData
                        accent: keyboard.accent
                        reducedMotion: keyboard.reducedMotion
                        onClicked: keyboard.enter(text)
                    }
                }
            }
        }
        Row {
            spacing: 12
            TvButton {
                text: keyboard.shifted ? "abc" : "ABC"
                width: 130
                onClicked: keyboard.shifted = !keyboard.shifted
            }
            TvButton {
                text: keyboard.symbols ? "Letters" : "123 #"
                width: 150
                onClicked: keyboard.symbols = !keyboard.symbols
            }
            TvButton {
                text: "Space"
                width: 300
                onClicked: keyboard.enter(" ")
            }
            TvButton {
                text: "⌫"
                width: 120
                onClicked: if (keyboard.target) {
                    let p = keyboard.target.cursorPosition;
                    keyboard.target.remove(Math.max(0, p - 1), p);
                }
            }
            TvButton {
                text: "Done"
                width: 200
                primary: true
                accent: keyboard.accent
                onClicked: keyboard.finish()
            }
        }
    }
    visible: false
}
