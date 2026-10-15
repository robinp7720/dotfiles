import QtQuick
import QtQuick.Controls
import QtQuick.Window

Window {
    id: window
    width: 1600
    height: 900
    minimumWidth: 960
    minimumHeight: 540
    title: "TV Home"
    color: "#0a1018"
    onClosing: close => {
        close.accepted = false;
        send({
            type: "exit"
        });
    }
    property var bridge: backend
    property var app: bridge.state
    property var lib: app.library || ({})
    property color accent: app.theme ? app.theme.primary : "#83d2e3"
    property bool reduced: app.reducedMotion || false
    property string page: lib.page || "home"
    property var shelves: []
    property var profiles: []
    property var detail: ({})
    property var hero: ({})
    property int seasonIndex: 0
    property var seasonMemory: ({})
    onDetailChanged: if (detail.kind === "series")
        seasonIndex = seasonMemory[String(detail.id)] || 0
    property int lastSerial: -1
    property bool focusPending: true
    property string selectedUser: ""
    property bool serverSettings: false
    property string clockText: Qt.formatTime(new Date(), "HH:mm")
    property real scrollTarget: 0
    property string oldBackdrop: ""
    property string newBackdrop: ""
    property int appHover: 0
    readonly property bool inLibrary: lib.authenticated || false
    readonly property var feature: page === "detail" ? detail : hero
    readonly property string backdrop: (lib.assets || {})[feature.backdrop || ""] || (lib.assets || {})[feature.art || ""] || ""
    function send(a) {
        bridge.send(a);
    }
    function same(a, b) {
        return JSON.stringify(a) === JSON.stringify(b);
    }
    function sync() {
        if (!same(shelves, lib.rows || []))
            shelves = lib.rows || [];
        if (!same(profiles, lib.profiles || []))
            profiles = lib.profiles || [];
        if (!same(detail, lib.detail || {}))
            detail = lib.detail || {};
        if (lastSerial !== lib.serial) {
            lastSerial = lib.serial;
            focusPending = true;
            scrollTarget = 0;
            keyboard.visible = false;
            hero = {};
        }
        if (page === "home" && !hero.id && shelves.length && shelves[0].items.length) {
            hero = shelves[0].items[0];
            requestArt(hero);
        }
        if (focusPending && !lib.loading) {
            focusTimer.restart();
        }
    }
    function requestArt(media) {
        if (media)
            send({
                type: "artwork",
                paths: [media.art || "", media.backdrop || ""]
            });
    }
    function openTitle(media) {
        hero = media;
        send({
            type: "detail",
            kind: media.kind,
            id: media.id
        });
    }
    function highlight(media, item) {
        hero = media;
        artTimer.restart();
        ensureVisible(item);
    }
    function ensureVisible(item) {
        if (!item)
            return;
        let ancestor = item;
        while (ancestor && ancestor !== scroll.contentItem)
            ancestor = ancestor.parent;
        if (!ancestor)
            return;
        let p = item.mapToItem(scroll.contentItem, 0, 0);
        if (p.y < scroll.contentY + 140)
            scrollTarget = Math.max(0, p.y - 145);
        else if (p.y + item.height > scroll.contentY + scroll.height - 45)
            scrollTarget = Math.min(scroll.contentHeight - scroll.height, p.y + item.height - scroll.height + 55);
    }
    function focusables(parent, result) {
        if (!parent.visible || !parent.enabled || parent.opacity === 0)
            return;
        if (parent.activeFocusOnTab && parent.width > 0 && parent.height > 0)
            result.push(parent);
        for (let i = 0; i < parent.children.length; i++)
            focusables(parent.children[i], result);
    }
    function firstFocus(parent) {
        let list = [];
        focusables(parent, list);
        if (list.length) {
            list[0].forceActiveFocus();
            ensureVisible(list[0]);
        }
    }
    function navigate(key) {
        if (key === "back") {
            if (keyboard.visible) {
                keyboard.finish();
                return;
            }
            send({
                type: page === "home" && !app.playing && !app.activeApp ? "exit" : "back"
            });
            return;
        }
        if (key === "release") {
            let f = window.activeFocusItem;
            if (f && f.released)
                f.released();
            send({
                type: "powerRelease"
            });
            return;
        }
        let current = window.activeFocusItem;
        if (key === "press") {
            if (current && current.toggle) {
                current.toggle();
                return;
            }
            if (current && current.popup) {
                current.popup.open();
                return;
            }
            if (current && current.openKeyboard) {
                current.openKeyboard();
                return;
            }
            if (current && current.pressed)
                current.pressed();
            if (current && current.clicked && !current.hold)
                current.clicked();
            else if (current && current.chosen)
                current.chosen(current.media);
            return;
        }
        if (current && current.gridOwner && current.gridOwner.navigate(key))
            return;
        let list = [];
        focusables(keyboard.visible ? keyboard : scene, list);
        if (!current) {
            if (list.length)
                list[0].forceActiveFocus();
            return;
        }
        let c = current.mapToItem(scene, current.width / 2, current.height / 2), best = null, score = 1e12;
        for (let item of list) {
            if (item === current)
                continue;
            let p = item.mapToItem(scene, item.width / 2, item.height / 2), dx = p.x - c.x, dy = p.y - c.y;
            let along = key === "left" ? -dx : key === "right" ? dx : key === "up" ? -dy : dy;
            let across = key === "left" || key === "right" ? Math.abs(dy) : Math.abs(dx);
            if (along < 8)
                continue;
            // Prefer the same shelf/column before moving diagonally.
            let s = along + across * 3 + across * across / Math.max(40, along);
            if (s < score) {
                score = s;
                best = item;
            }
        }
        if (best) {
            best.forceActiveFocus();
            ensureVisible(best);
        }
    }
    function openKeyboard(field) {
        keyboard.open(field);
        Qt.callLater(function () {
            firstFocus(keyboard);
        });
    }
    function login() {
        send({
            type: "login",
            server: server.text,
            credentials: {
                username: username.text,
                password: password.text
            },
            remember: remember.checked
        });
        password.text = "";
        keyboard.visible = false;
    }
    onPageChanged: pageArrival.restart()
    onBackdropChanged: {
        if (backdrop !== newBackdrop) {
            oldBackdrop = inLibrary ? newBackdrop : "";
            newBackdrop = backdrop;
            backdropFade.restart();
        }
    }
    Connections {
        target: window.bridge
        function onStateChanged() {
            sync();
        }
        function onInput(key) {
            navigate(key);
        }
    }
    Shortcut {
        sequences: [StandardKey.Quit]
        onActivated: send({
            type: "exit"
        })
    }
    Component.onCompleted: sync()
    Timer {
        id: focusTimer
        interval: 80
        onTriggered: {
            focusPending = false;
            if (page === "home")
                heroButton.forceActiveFocus();
            else if (page === "login") {
                if (profiles.length)
                    firstFocus(profileRow);
                else
                    username.forceActiveFocus();
            } else if (page === "detail")
                firstFocus(detailArea);
            else if (page === "browse") {
                if (catalogue.count)
                    catalogue.restoreFocus();
                else
                    firstFocus(browseHeader);
            } else if (page === "search")
                searchField.forceActiveFocus();
            else
                firstFocus(accountArea);
        }
    }
    Timer {
        id: artTimer
        interval: 180
        onTriggered: requestArt(hero)
    }
    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: clockText = Qt.formatTime(new Date(), "HH:mm")
    }
    Item {
        id: scene
        width: 1920
        height: 1080
        anchors.centerIn: parent
        scale: Math.min(window.width / width, window.height / height)
        Ambient {
            anchors.fill: parent
            accent: window.accent
            drift: appHover
            reducedMotion: reduced
        }
        Image {
            id: previousImage
            anchors.fill: parent
            source: oldBackdrop
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            opacity: newImage.source ? 1 : 0.6
            sourceSize.width: 1920
        }
        Image {
            id: newImage
            anchors.fill: parent
            source: newBackdrop
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize.width: 1920
            opacity: 0
            onStatusChanged: if (status === Image.Ready)
                backdropFade.restart()
        }
        NumberAnimation {
            id: backdropFade
            target: newImage
            property: "opacity"
            from: 0
            to: newBackdrop ? 0.8 : 0
            duration: reduced ? 0 : 600
            easing.type: Easing.OutCubic
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: "#ff0a1018"
                }
                GradientStop {
                    position: .35
                    color: "#c70a1018"
                }
                GradientStop {
                    position: 1
                    color: "#180a1018"
                }
            }
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: "#600a1018"
                }
                GradientStop {
                    position: .4
                    color: "#000a1018"
                }
                GradientStop {
                    position: .85
                    color: "#f50a1018"
                }
                GradientStop {
                    position: 1
                    color: "#ff0a1018"
                }
            }
        }
        // A soft vignette leaves every focus target readable over artwork.
        Rectangle {
            anchors.fill: parent
            color: "#0a1018"
            opacity: page === "browse" ? 0.38 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: reduced ? 0 : 180
                }
            }
        }
        Item {
            id: header
            x: 168
            y: 55
            width: 1664
            height: 66
            Row {
                spacing: 14
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    text: "◉"
                    font.pixelSize: 37
                    color: accent
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    text: "oblecto"
                    font.family: "Inter"
                    font.pixelSize: 33
                    font.weight: Font.DemiBold
                    font.letterSpacing: -1
                    color: "#f1f6f8"
                }
                Rectangle {
                    width: 1
                    height: 27
                    color: "#41545e"
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    text: page === "home" ? "TV HOME" : page === "detail" ? "YOUR LIBRARY" : page.toUpperCase()
                    font.family: "Inter"
                    font.pixelSize: 17
                    font.letterSpacing: 3
                    color: "#adc0c8"
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 28
                TvButton {
                    objectName: "exitTv"
                    text: "Exit TV"
                    icon: "↗"
                    width: 174
                    height: 56
                    accent: window.accent
                    reducedMotion: reduced
                    onClicked: send({
                        type: "exit"
                    })
                }
                Text {
                    text: lib.account || "Make yourself at home"
                    font.family: "Inter"
                    font.pixelSize: 21
                    color: "#becdd3"
                    anchors.verticalCenter: parent.verticalCenter
                }
                Rectangle {
                    width: 40
                    height: 40
                    radius: 20
                    color: "#344d59"
                    visible: inLibrary
                    Text {
                        anchors.centerIn: parent
                        text: (lib.account || "?").substring(0, 1).toUpperCase()
                        font.pixelSize: 20
                        color: "#d6ebf0"
                    }
                }
                Text {
                    text: clockText
                    font.family: "Inter"
                    font.pixelSize: 25
                    color: "#e8f0f3"
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
        Column {
            id: rail
            objectName: "rail"
            x: 35
            y: 278
            spacing: 19
            z: 5
            Repeater {
                model: [
                    {
                        icon: "⌂",
                        name: "Home",
                        route: "home"
                    },
                    {
                        icon: "⌕",
                        name: "Search",
                        route: "search"
                    },
                    {
                        icon: "▣",
                        name: "Movies",
                        route: "movie"
                    },
                    {
                        icon: "▤",
                        name: "Series",
                        route: "series"
                    },
                    {
                        icon: "♡",
                        name: "Watchlist",
                        route: "saved"
                    },
                    {
                        icon: "⚙",
                        name: "Settings",
                        route: "account"
                    }
                ]
                TvButton {
                    required property var modelData
                    compact: true
                    width: 72
                    height: 72
                    icon: modelData.icon
                    accent: window.accent
                    reducedMotion: reduced
                    selected: (page === modelData.route) || (page === "browse" && lib.browseKind === modelData.route)
                    Accessible.name: modelData.name
                    onClicked: {
                        if (modelData.route === "movie" || modelData.route === "series")
                            send({
                                type: "browse",
                                kind: modelData.route,
                                list: "all"
                            });
                        else if (modelData.route === "saved")
                            send({
                                type: "browse",
                                kind: "movie",
                                list: "watchlist"
                            });
                        else
                            send({
                                type: modelData.route
                            });
                    }
                    Text {
                        anchors.left: parent.right
                        anchors.leftMargin: 15
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.name
                        color: "#f1f7fa"
                        font.pixelSize: 22
                        visible: parent.activeFocus
                        style: Text.Outline
                        styleColor: "#101923"
                    }
                }
            }
        }
        MediaGrid {
            id: catalogue
            objectName: "catalogue"
            x: 168
            y: 315
            width: 1664
            height: 710
            visible: page === "browse"
            items: page === "browse" && shelves.length ? shelves[0].items : []
            assets: lib.assets || {}
            accent: window.accent
            reducedMotion: reduced
            loading: !!lib.loading
            moreLoading: !!lib.moreLoading
            cursor: lib.cursor || ""
            error: lib.moreError || ""
            routeSerial: lib.serial || 0
            restoreIndex: lib.browseIndex || 0
            restoreScroll: lib.browseScroll || 0
            onChosen: media => openTitle(media)
            onHighlighted: media => {
                hero = media;
                artTimer.restart();
            }
            onLoadMore: send({
                type: "more"
            })
            onPositionSaved: (index, scroll) => send({
                    type: "browse_position",
                    index: index,
                    scroll: scroll
                })
            onArtworkRequested: paths => send({
                    type: "artwork",
                    paths: paths
                })
        }
        Flickable {
            id: scroll
            visible: page !== "browse"
            objectName: "scroll"
            x: 150
            y: 140
            width: 1720
            height: 875
            clip: true
            contentWidth: width
            contentHeight: content.height + 80
            boundsBehavior: Flickable.StopAtBounds
            interactive: !keyboard.visible
            contentY: scrollTarget
            Behavior on contentY {
                NumberAnimation {
                    duration: reduced ? 0 : 280
                    easing.type: Easing.OutCubic
                }
            }
            Item {
                id: content
                x: 18
                width: 1664
                height: page === "login" ? 850 : page === "search" ? 770 : page === "account" ? 830 : page === "detail" ? detailArea.height + (episodeSection.visible ? episodeSection.height : 0) : shelfColumn.y + shelfColumn.height
                Item {
                    id: heroArea
                    width: parent.width
                    height: inLibrary ? 430 : 465
                    visible: page === "home"
                    Text {
                        x: 0
                        y: 51
                        text: inLibrary ? "TONIGHT, PICK UP A GREAT STORY" : "THE BIG SCREEN IS YOURS"
                        font.family: "Inter"
                        font.pixelSize: 18
                        font.weight: Font.DemiBold
                        font.letterSpacing: 4
                        color: accent
                    }
                    Text {
                        x: 0
                        y: 99
                        width: 1030
                        height: inLibrary ? 165 : 230
                        text: inLibrary ? (hero.title || "Settle into something good.") : "Your evening.\nYour way."
                        font.family: "Inter"
                        font.pixelSize: inLibrary ? 76 : 100
                        font.weight: Font.DemiBold
                        font.letterSpacing: -3
                        lineHeight: 0.95
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        color: "#f4f7f9"
                    }
                    Text {
                        x: 2
                        y: inLibrary ? 275 : 342
                        width: 870
                        text: inLibrary ? (hero.overview || hero.subtitle || "Films, series, and the stories you keep coming back to.") : "Films, games, and one more episode. All in one place."
                        font.family: "Inter"
                        font.pixelSize: 25
                        lineHeight: 1.25
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        color: "#adbec8"
                    }
                    TvButton {
                        id: heroButton
                        objectName: "heroButton"
                        x: 0
                        y: inLibrary ? 365 : 426
                        text: inLibrary && hero.id ? "Explore title" : "Open Oblecto"
                        icon: "▶"
                        primary: true
                        accent: window.accent
                        reducedMotion: reduced
                        onClicked: inLibrary && hero.id ? openTitle(hero) : send({
                            type: "open"
                        })
                    }
                    Text {
                        x: heroButton.width + 29
                        y: heroButton.y + 22
                        text: inLibrary ? (hero.subtitle || "") : "Your personal cinema"
                        font.family: "Inter"
                        font.pixelSize: 21
                        color: "#91a7b2"
                    }
                }
                Row {
                    id: launchers
                    y: inLibrary ? 480 : 560
                    spacing: 22
                    visible: page === "home"
                    Repeater {
                        model: [
                            {
                                name: "Oblecto",
                                tag: "YOUR PERSONAL CINEMA",
                                description: "The stories you love, together.",
                                symbol: "◉",
                                color: "#213f4a",
                                route: "open"
                            },
                            {
                                name: "Steam",
                                tag: "READY, PLAYER ONE",
                                description: "Jump into your next adventure.",
                                symbol: "✦",
                                color: "#28324b",
                                route: "steam"
                            },
                            {
                                name: "Kodi",
                                tag: "MORE TO EXPLORE",
                                description: "A world of media, on your terms.",
                                symbol: "◇",
                                color: "#283b45",
                                route: "kodi"
                            }
                        ]
                        Item {
                            id: launch
                            required property var modelData
                            required property int index
                            width: 540
                            height: inLibrary ? 124 : 244
                            activeFocusOnTab: true
                            scale: activeFocus ? 1.025 : 1
                            z: activeFocus ? 2 : 0
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData.name
                            property bool hold: false
                            signal clicked
                            onClicked: modelData.route === "open" ? send({
                                type: "open"
                            }) : send({
                                type: "app",
                                name: modelData.route
                            })
                            onActiveFocusChanged: if (activeFocus)
                                appHover = index
                            Behavior on scale {
                                NumberAnimation {
                                    duration: reduced ? 0 : 180
                                    easing.type: Easing.OutCubic
                                }
                            }
                            Rectangle {
                                anchors.fill: parent
                                radius: 22
                                color: modelData.color
                                border.width: launch.activeFocus ? 3 : 1
                                border.color: launch.activeFocus ? "#f1f7fa" : "#355a6a77"
                            }
                            Text {
                                x: inLibrary ? 28 : 35
                                y: inLibrary ? 30 : 30
                                text: modelData.symbol
                                font.pixelSize: inLibrary ? 52 : 67
                                color: index === 0 ? accent : index === 1 ? "#b3c2f1" : "#a4d7df"
                            }
                            Text {
                                x: inLibrary ? 105 : 35
                                y: inLibrary ? 26 : 130
                                text: modelData.name
                                font.family: "Inter"
                                font.pixelSize: inLibrary ? 30 : 39
                                font.weight: Font.DemiBold
                                color: "#f1f6f8"
                            }
                            Text {
                                x: inLibrary ? 106 : 36
                                y: inLibrary ? 73 : 188
                                text: inLibrary ? modelData.tag : modelData.description
                                font.family: "Inter"
                                font.pixelSize: inLibrary ? 14 : 20
                                font.letterSpacing: inLibrary ? 2 : 0
                                color: "#aec0cc"
                            }
                            Text {
                                anchors.right: parent.right
                                anchors.rightMargin: 30
                                y: inLibrary ? 40 : 37
                                text: "↗"
                                font.pixelSize: 35
                                color: launch.activeFocus ? "#ffffff" : "#8aa8b9"
                            }
                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onEntered: launch.forceActiveFocus()
                                onClicked: launch.clicked()
                            }
                            Keys.onReturnPressed: clicked()
                            Keys.onEnterPressed: clicked()
                            Keys.onSpacePressed: clicked()
                        }
                    }
                }
                Item {
                    id: browseHeader
                    parent: scene
                    x: 168
                    y: 150
                    width: 1664
                    height: 155
                    visible: page === "browse"
                    Text {
                        y: 8
                        text: lib.title || "Your library"
                        width: 1600
                        elide: Text.ElideRight
                        font.family: "Inter"
                        font.pixelSize: 52
                        font.weight: Font.DemiBold
                        font.letterSpacing: -2
                        color: "#f3f7fa"
                    }
                    Row {
                        y: 85
                        spacing: 16
                        Repeater {
                            model: [
                                {
                                    label: "Movies",
                                    kind: "movie",
                                    list: lib.browseList || "all"
                                },
                                {
                                    label: "Series",
                                    kind: "series",
                                    list: lib.browseList || "all"
                                },
                                {
                                    label: "All titles",
                                    kind: lib.browseKind || "movie",
                                    list: "all"
                                },
                                {
                                    label: "Watchlist",
                                    kind: lib.browseKind || "movie",
                                    list: "watchlist"
                                },
                                {
                                    label: "Favourites",
                                    kind: lib.browseKind || "movie",
                                    list: "favourites"
                                }
                            ]
                            TvButton {
                                required property var modelData
                                text: modelData.label
                                height: 56
                                selected: lib.browseKind === modelData.kind && lib.browseList === modelData.list
                                accent: window.accent
                                reducedMotion: reduced
                                onClicked: send({
                                    type: "browse",
                                    kind: modelData.kind,
                                    list: modelData.list
                                })
                            }
                        }
                    }
                }
                Item {
                    id: detailArea
                    width: parent.width
                    height: detail.kind === "series" ? 585 : 760
                    visible: page === "detail" && !lib.loading
                    Image {
                        x: 1280
                        y: 85
                        width: 300
                        height: 450
                        source: (lib.assets || {})[detail.art || ""] || ""
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        opacity: status === Image.Ready ? 0.9 : 0
                        Behavior on opacity {
                            NumberAnimation {
                                duration: reduced ? 0 : 320
                            }
                        }
                    }
                    Text {
                        y: 38
                        text: detail.kind === "series" ? "A STORY TO GET LOST IN" : detail.resumable ? "WELCOME BACK TO THE STORY" : "MAKE IT A MOVIE NIGHT"
                        font.family: "Inter"
                        font.pixelSize: 18
                        font.letterSpacing: 4
                        color: accent
                    }
                    Text {
                        y: 86
                        width: 1130
                        text: detail.title || ""
                        font.family: "Inter"
                        font.pixelSize: 79
                        font.weight: Font.DemiBold
                        font.letterSpacing: -2
                        color: "#f3f7fa"
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                    Text {
                        y: 282
                        text: detail.subtitle || ""
                        font.family: "Inter"
                        font.pixelSize: 23
                        color: accent
                    }
                    Text {
                        y: 329
                        width: 1090
                        height: 112
                        text: detail.overview || "No synopsis available."
                        font.family: "Inter"
                        font.pixelSize: 27
                        lineHeight: 1.3
                        color: "#bbcbd3"
                        wrapMode: Text.Wrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                    }
                    Row {
                        y: 485
                        spacing: 17
                        TvButton {
                            text: detail.resumable ? "Resume · " + detail.resume : "Play"
                            icon: "▶"
                            primary: true
                            accent: window.accent
                            reducedMotion: reduced
                            visible: detail.kind !== "series" && (detail.files || []).length > 0
                            onClicked: send({
                                type: "play",
                                source: sourcePicker.currentIndex
                            })
                        }
                        TvButton {
                            text: "Start over"
                            visible: detail.resumable || false
                            onClicked: send({
                                type: "play",
                                source: sourcePicker.currentIndex,
                                restart: true
                            })
                            reducedMotion: reduced
                        }
                        TvButton {
                            text: detail.favourite ? "♥ Favourite" : "♡ Favourite"
                            onClicked: send({
                                type: "flag",
                                flag: "favourite",
                                enabled: !detail.favourite
                            })
                            reducedMotion: reduced
                        }
                        TvButton {
                            text: detail.watchlist ? "✓ In watchlist" : "+ Watchlist"
                            visible: detail.kind !== "episode"
                            onClicked: send({
                                type: "flag",
                                flag: "watchlist",
                                enabled: !detail.watchlist
                            })
                            reducedMotion: reduced
                        }
                        TvButton {
                            text: detail.watched ? "✓ Watched" : "Mark watched"
                            visible: detail.kind !== "series"
                            onClicked: send({
                                type: "flag",
                                flag: "watched",
                                enabled: !detail.watched
                            })
                            reducedMotion: reduced
                        }
                    }
                    Row {
                        y: 590
                        spacing: 20
                        visible: detail.kind !== "series"
                        TvButton {
                            id: sourcePicker
                            objectName: "sourcePicker"
                            property int currentIndex: 0
                            property var sources: detail.files || []
                            onSourcesChanged: currentIndex = 0
                            width: 890
                            text: "Source " + (currentIndex + 1) + "/" + sources.length + "  ·  " + (sources[currentIndex] ? sources[currentIndex].label : "")
                            visible: sources.length > 1
                            onClicked: currentIndex = (currentIndex + 1) % sources.length
                            reducedMotion: reduced
                        }
                        Text {
                            text: "No playable source is linked to this title."
                            font.pixelSize: 25
                            color: "#becdd4"
                            visible: (detail.files || []).length === 0
                        }
                        TvButton {
                            text: "Next episode →"
                            visible: !!detail.next
                            onClicked: send({
                                type: "detail",
                                kind: "episode",
                                id: detail.next
                            })
                        }
                    }
                }
                Column {
                    id: episodeSection
                    objectName: "episodeSection"
                    y: detailArea.height
                    width: 1664
                    spacing: 28
                    visible: page === "detail" && detail.kind === "series"
                    Flow {
                        width: parent.width
                        spacing: 14
                        Repeater {
                            model: episodeSection.visible ? shelves : []
                            TvButton {
                                required property var modelData
                                required property int index
                                text: modelData.title
                                height: 58
                                selected: seasonIndex === index
                                accent: window.accent
                                reducedMotion: reduced
                                onClicked: {
                                    seasonIndex = index;
                                    seasonMemory[String(detail.id)] = index;
                                }
                            }
                        }
                    }
                    Grid {
                        objectName: "episodeGrid"
                        columns: 4
                        columnSpacing: 27
                        rowSpacing: 22
                        Repeater {
                            model: episodeSection.visible && shelves[seasonIndex] ? shelves[seasonIndex].items : []
                            Poster {
                                required property var modelData
                                media: modelData
                                wide: true
                                assets: lib.assets || {}
                                accent: window.accent
                                reducedMotion: reduced
                                onChosen: media => openTitle(media)
                                onHighlighted: media => highlight(media, this)
                            }
                        }
                    }
                }
                Column {
                    id: shelfColumn
                    y: page === "home" ? (inLibrary ? 660 : 850) : page === "detail" ? detailArea.height : 270
                    width: parent.width
                    spacing: 36
                    visible: page === "home"
                    Repeater {
                        model: page === "home" && inLibrary ? shelves : []
                        Column {
                            id: shelf
                            required property var modelData
                            required property int index
                            width: shelfColumn.width
                            spacing: 25
                            Item {
                                width: parent.width
                                height: 50
                                Text {
                                    text: shelf.modelData.title
                                    anchors.verticalCenter: parent.verticalCenter
                                    font.family: "Inter"
                                    font.pixelSize: 29
                                    font.weight: Font.DemiBold
                                    color: "#e5eef2"
                                }
                                TvButton {
                                    anchors.right: parent.right
                                    width: 240
                                    height: 50
                                    text: "Browse all →"
                                    visible: shelf.modelData.title === "Something worth watching" || shelf.modelData.title === "One more episode"
                                    accent: window.accent
                                    reducedMotion: reduced
                                    onClicked: send({
                                        type: "browse",
                                        kind: shelf.modelData.items[0].kind,
                                        list: "all"
                                    })
                                }
                            }
                            Flickable {
                                id: rowScroll
                                width: parent.width
                                height: cards.height + 22
                                contentWidth: cards.width + 25
                                clip: false
                                boundsBehavior: Flickable.StopAtBounds
                                Row {
                                    id: cards
                                    spacing: 27
                                    y: 6
                                    Repeater {
                                        model: shelf.modelData.items
                                        Poster {
                                            required property var modelData
                                            media: modelData
                                            assets: lib.assets || {}
                                            accent: window.accent
                                            reducedMotion: reduced
                                            onChosen: media => openTitle(media)
                                            onHighlighted: media => {
                                                highlight(media, this);
                                                let p = x;
                                                if (p < rowScroll.contentX)
                                                    rowScroll.contentX = Math.max(0, p - 12);
                                                else if (p + width > rowScroll.contentX + rowScroll.width)
                                                    rowScroll.contentX = p + width - rowScroll.width + 25;
                                            }
                                        }
                                    }
                                }
                                Behavior on contentX {
                                    NumberAnimation {
                                        duration: reduced ? 0 : 260
                                        easing.type: Easing.OutCubic
                                    }
                                }
                            }
                        }
                    }
                    Text {
                        visible: !lib.loading && inLibrary && shelves.length === 0 && page !== "detail"
                        text: "Nothing here yet. Your next favourite is waiting to be discovered."
                        font.family: "Inter"
                        font.pixelSize: 27
                        color: "#9db3bf"
                        width: 1200
                        wrapMode: Text.Wrap
                    }
                }
                Item {
                    id: loginArea
                    visible: page === "login"
                    width: parent.width
                    height: 850
                    Text {
                        y: 52
                        text: "Who's watching?"
                        font.family: "Inter"
                        font.pixelSize: 76
                        font.weight: Font.DemiBold
                        font.letterSpacing: -3
                        color: "#f1f7f9"
                    }
                    Text {
                        y: 160
                        text: "A familiar face. A whole world of stories."
                        font.family: "Inter"
                        font.pixelSize: 27
                        color: "#a9beca"
                    }
                    Row {
                        id: profileRow
                        y: 244
                        spacing: 28
                        Repeater {
                            model: profiles
                            Item {
                                id: profile
                                required property var modelData
                                required property int index
                                width: 180
                                height: 180
                                activeFocusOnTab: true
                                property bool hold: false
                                signal clicked
                                Accessible.role: Accessible.Button
                                Accessible.name: modelData.name || modelData.username
                                onClicked: {
                                    selectedUser = modelData.username;
                                    username.text = selectedUser;
                                    if (modelData.passwordless)
                                        send({
                                            type: "login",
                                            server: server.text,
                                            credentials: {
                                                userId: modelData.id
                                            },
                                            remember: remember.checked
                                        });
                                    else
                                        password.forceActiveFocus();
                                }
                                Rectangle {
                                    width: 112
                                    height: 112
                                    radius: 34
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    color: profile.activeFocus ? accent : index % 2 === 0 ? "#375263" : "#4d4b68"
                                    border.width: profile.activeFocus ? 4 : 0
                                    border.color: "#ffffff"
                                    scale: profile.activeFocus ? 1.08 : 1
                                    Behavior on scale {
                                        NumberAnimation {
                                            duration: reduced ? 0 : 180
                                        }
                                    }
                                    Text {
                                        anchors.centerIn: parent
                                        text: (profile.modelData.name || profile.modelData.username || "?").substring(0, 1).toUpperCase()
                                        font.family: "Inter"
                                        font.pixelSize: 47
                                        font.weight: Font.Medium
                                        color: profile.activeFocus ? "#10232b" : "#e7f1f8"
                                    }
                                }
                                Text {
                                    y: 137
                                    width: parent.width
                                    text: modelData.name || modelData.username
                                    horizontalAlignment: Text.AlignHCenter
                                    font.family: "Inter"
                                    font.pixelSize: 25
                                    color: profile.activeFocus ? "#ffffff" : "#afc3d0"
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onEntered: profile.forceActiveFocus()
                                    onClicked: profile.clicked()
                                }
                                Keys.onReturnPressed: clicked()
                                Keys.onEnterPressed: clicked()
                            }
                        }
                    }
                    Row {
                        y: profiles.length ? 460 : 250
                        spacing: 20
                        Column {
                            spacing: 12
                            Text {
                                text: "USERNAME"
                                font.pixelSize: 15
                                font.letterSpacing: 3
                                color: "#99b2bf"
                            }
                            TextField {
                                id: username
                                objectName: "username"
                                width: 440
                                height: 72
                                placeholderTextColor: "#91aab9"
                                placeholderText: "Your username"
                                font.pixelSize: 26
                                font.family: "Inter"
                                color: "#f1f7fa"
                                leftPadding: 22
                                activeFocusOnTab: true
                                Accessible.name: "Username"
                                function openKeyboard() {
                                    window.openKeyboard(username);
                                }
                                Keys.onReturnPressed: openKeyboard()
                                background: Rectangle {
                                    radius: 18
                                    color: "#243744"
                                    border.width: username.activeFocus ? 3 : 1
                                    border.color: username.activeFocus ? accent : "#48606b"
                                }
                            }
                        }
                        Column {
                            spacing: 12
                            Text {
                                text: "PASSWORD"
                                font.pixelSize: 15
                                font.letterSpacing: 3
                                color: "#99b2bf"
                            }
                            TextField {
                                id: password
                                objectName: "password"
                                width: 440
                                height: 72
                                placeholderTextColor: "#91aab9"
                                placeholderText: "Your password"
                                echoMode: TextInput.Password
                                font.pixelSize: 26
                                font.family: "Inter"
                                color: "#f1f7fa"
                                leftPadding: 22
                                activeFocusOnTab: true
                                Accessible.name: "Password"
                                function openKeyboard() {
                                    window.openKeyboard(password);
                                }
                                onAccepted: window.login()
                                background: Rectangle {
                                    radius: 18
                                    color: "#243744"
                                    border.width: password.activeFocus ? 3 : 1
                                    border.color: password.activeFocus ? accent : "#48606b"
                                }
                            }
                        }
                        TvButton {
                            text: lib.loading ? "Connecting…" : "Sign in →"
                            y: 29
                            height: 72
                            primary: true
                            accent: window.accent
                            enabled: !lib.loading
                            onClicked: window.login()
                            reducedMotion: reduced
                        }
                    }
                    Row {
                        y: profiles.length ? 600 : 390
                        spacing: 28
                        CheckBox {
                            id: remember
                            objectName: "remember"
                            text: "Remember me"
                            checked: true
                            font.family: "Inter"
                            font.pixelSize: 23
                            activeFocusOnTab: true
                            palette.windowText: "#bfd1db"
                        }
                        TvButton {
                            text: "Server settings"
                            height: 58
                            onClicked: {
                                serverSettings = !serverSettings;
                                if (serverSettings)
                                    server.forceActiveFocus();
                            }
                            reducedMotion: reduced
                        }
                    }
                    Row {
                        y: profiles.length ? 710 : 510
                        spacing: 20
                        visible: serverSettings
                        TextField {
                            id: server
                            width: 900
                            height: 70
                            text: lib.server || ""
                            font.pixelSize: 23
                            color: "#e9f5fb"
                            leftPadding: 20
                            activeFocusOnTab: true
                            function openKeyboard() {
                                window.openKeyboard(server);
                            }
                            Keys.onReturnPressed: openKeyboard()
                            background: Rectangle {
                                radius: 16
                                color: "#263c49"
                                border.width: server.activeFocus ? 3 : 1
                                border.color: accent
                            }
                        }
                        TvButton {
                            text: "Connect"
                            onClicked: send({
                                type: "profiles",
                                server: server.text
                            })
                            reducedMotion: reduced
                        }
                    }
                }
                Item {
                    id: searchArea
                    visible: page === "search"
                    width: parent.width
                    height: 700
                    Text {
                        y: 55
                        text: "Find your next favourite."
                        font.family: "Inter"
                        font.pixelSize: 72
                        font.weight: Font.DemiBold
                        font.letterSpacing: -2
                        color: "#f1f6f9"
                    }
                    Text {
                        y: 160
                        text: "A film you've been meaning to watch. A series worth staying in for."
                        font.family: "Inter"
                        font.pixelSize: 27
                        color: "#afc3cf"
                    }
                    TextField {
                        id: searchField
                        objectName: "searchField"
                        y: 270
                        width: 1250
                        height: 86
                        placeholderTextColor: "#91aab9"
                        placeholderText: "Search your library"
                        font.pixelSize: 32
                        font.family: "Inter"
                        color: "#f2f7fa"
                        leftPadding: 26
                        activeFocusOnTab: true
                        Accessible.name: "Search your library"
                        function openKeyboard() {
                            window.openKeyboard(searchField);
                        }
                        onAccepted: send({
                            type: "browse",
                            kind: "movie",
                            query: text,
                            list: "all"
                        })
                        background: Rectangle {
                            radius: 22
                            color: "#263c49"
                            border.width: searchField.activeFocus ? 3 : 1
                            border.color: accent
                        }
                    }
                    Row {
                        y: 400
                        spacing: 20
                        TvButton {
                            text: "Search movies"
                            primary: true
                            accent: window.accent
                            onClicked: send({
                                type: "browse",
                                kind: "movie",
                                query: searchField.text,
                                list: "all"
                            })
                        }
                        TvButton {
                            text: "Search series"
                            onClicked: send({
                                type: "browse",
                                kind: "series",
                                query: searchField.text,
                                list: "all"
                            })
                        }
                    }
                }
                Column {
                    id: accountArea
                    visible: page === "account"
                    y: 50
                    width: parent.width
                    spacing: 30
                    Text {
                        text: "Make yourself comfortable."
                        font.family: "Inter"
                        font.pixelSize: 65
                        font.weight: Font.DemiBold
                        color: "#eef5f8"
                        font.letterSpacing: -2
                    }
                    Text {
                        text: (lib.account || "Guest") + "  ·  " + (lib.server || "")
                        font.family: "Inter"
                        font.pixelSize: 24
                        color: "#a8bfcb"
                    }
                    Row {
                        spacing: 18
                        TvButton {
                            text: "Refresh library"
                            primary: true
                            accent: window.accent
                            onClicked: send({
                                type: "home"
                            })
                        }
                        TvButton {
                            text: "Sign in"
                            visible: !inLibrary
                            onClicked: send({
                                type: "open"
                            })
                        }
                        TvButton {
                            text: "Sign out"
                            visible: inLibrary
                            onClicked: send({
                                type: "logout"
                            })
                        }
                        TvButton {
                            text: reduced ? "Motion: reduced" : "Motion: full"
                            onClicked: send({
                                type: "motion",
                                reduced: !reduced
                            })
                        }
                    }
                    Text {
                        text: "Playback follows your quality, language, and spoiler preferences in Oblecto."
                        font.family: "Inter"
                        font.pixelSize: 24
                        color: "#93aebb"
                    }
                    Rectangle {
                        width: 1600
                        height: 1
                        color: "#334c59"
                    }
                    Text {
                        text: "When the evening's done"
                        font.family: "Inter"
                        font.pixelSize: 34
                        color: "#dceaf2"
                    }
                    Row {
                        spacing: 18
                        TvButton {
                            text: "Sleep · hold 2s"
                            hold: true
                            progress: activeFocus ? (app.holdProgress || 0) : 0
                            onPressed: send({
                                type: "powerPress",
                                name: "sleep"
                            })
                            onReleased: send({
                                type: "powerRelease"
                            })
                        }
                        TvButton {
                            text: "Shut down · hold 2s"
                            hold: true
                            progress: activeFocus ? (app.holdProgress || 0) : 0
                            onPressed: send({
                                type: "powerPress",
                                name: "shutdown"
                            })
                            onReleased: send({
                                type: "powerRelease"
                            })
                        }
                        TvButton {
                            text: "Return to desktop"
                            onClicked: send({
                                type: "exit"
                            })
                        }
                    }
                    Text {
                        text: app.controller || "Keyboard & remote"
                        font.family: "Inter"
                        font.pixelSize: 22
                        color: "#91aab8"
                    }
                }
            }
        }
        Rectangle {
            x: 150
            y: 1005
            width: 1720
            height: 75
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: "#000a1018"
                }
                GradientStop {
                    position: .35
                    color: "#ff0a1018"
                }
            }
        }
        Row {
            x: 170
            y: 1030
            spacing: 26
            Repeater {
                model: [
                    {
                        key: "A",
                        label: "Select"
                    },
                    {
                        key: "B",
                        label: page === "home" ? (app.playing || app.activeApp ? "Return to app" : "Desktop") : "Back"
                    },
                    {
                        key: "⌂",
                        label: "Hold Guide for home"
                    },
                    {
                        key: "⧉",
                        label: "View / Select exits TV"
                    }
                ]
                Row {
                    required property var modelData
                    spacing: 9
                    Rectangle {
                        width: 27
                        height: 27
                        radius: 14
                        color: "#30444e"
                        Text {
                            anchors.centerIn: parent
                            text: modelData.key
                            font.pixelSize: 16
                            font.weight: Font.DemiBold
                            color: "#cfdee7"
                        }
                    }
                    Text {
                        text: modelData.label
                        font.family: "Inter"
                        font.pixelSize: 18
                        color: "#8da7b6"
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
        TvButton {
            x: 1480
            y: 1016
            width: 350
            height: 48
            text: app.playing ? "Return to playback" : app.activeApp ? "Return to " + app.activeApp : ""
            visible: app.playing || !!app.activeApp
            onClicked: send({
                type: "resumeApp"
            })
        }
        Text {
            x: 1510
            y: 1030
            text: "TV MODE  /  PREVIEW"
            font.family: "Inter"
            font.pixelSize: 15
            font.letterSpacing: 2
            color: "#678592"
            visible: !!app.preview && !app.playing && !app.activeApp
        }
        Rectangle {
            id: toast
            x: 260
            y: 920
            width: 1400
            height: 74
            radius: 20
            color: "#f228414f"
            border.color: "#537282"
            visible: !!lib.message
            z: 9
            Text {
                anchors.fill: parent
                anchors.margins: 20
                text: lib.message || ""
                font.family: "Inter"
                font.pixelSize: 23
                color: "#e3f2fa"
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
            MouseArea {
                anchors.fill: parent
                onClicked: toast.visible = false
            }
        }
        Rectangle {
            x: 168
            y: 132
            height: 3
            width: lib.loading ? 1664 : 0
            color: accent
            opacity: lib.loading ? 0.8 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: 180
                }
            }
        }
        Rectangle {
            anchors.fill: parent
            color: "#90080e16"
            visible: keyboard.visible
            z: 19
            MouseArea {
                anchors.fill: parent
                onClicked: keyboard.finish()
            }
        }
        Keyboard {
            id: keyboard
            objectName: "keyboard"
            x: 420
            y: 640
            z: 20
            accent: window.accent
            reducedMotion: reduced
        }
        ParallelAnimation {
            id: pageArrival
            NumberAnimation {
                target: content
                property: "opacity"
                from: 0
                to: 1
                duration: reduced ? 0 : 320
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: content
                property: "y"
                from: 24
                to: 0
                duration: reduced ? 0 : 320
                easing.type: Easing.OutCubic
            }
        }
        focus: true
        Keys.onPressed: event => {
            let keys = {};
            keys[Qt.Key_Left] = "left";
            keys[Qt.Key_Right] = "right";
            keys[Qt.Key_Up] = "up";
            keys[Qt.Key_Down] = "down";
            keys[Qt.Key_Escape] = "back";
            if (keys[event.key]) {
                navigate(keys[event.key]);
                event.accepted = true;
            } else if (event.key === Qt.Key_Home) {
                send({
                    type: "home"
                });
                event.accepted = true;
            }
        }
    }
}
