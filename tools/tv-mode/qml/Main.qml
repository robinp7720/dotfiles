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
    color: "#141414"
    onClosing: close => {
        close.accepted = false;
        send({
            type: "exit"
        });
    }
    property var bridge: backend
    property var app: bridge.state
    readonly property bool playerShowing: !!app.playerView
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
    readonly property bool inLibrary: lib.authenticated || false
    readonly property var feature: page === "detail" ? detail : hero
    readonly property string backdrop: page !== "home" && page !== "detail" ? "" : (lib.assets || {})[feature.backdrop || ""] || (lib.assets || {})[feature.art || ""] || ""
    function send(a) {
        bridge.send(a);
    }
    // The native host starts this once the window has presented its first frame.
    // Returning from Steam or playback must not replay the session entrance.
    function enterSession() {
        sessionMotion.shown = true;
    }
    Motion {
        id: sessionMotion
        objectName: "sessionMotion"
        reducedMotion: reduced
        duration: 360
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
            keyboard.opened = false;
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
        if (!parent.visible || !parent.enabled)
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
        if (playerShowing) {
            playbackView.navigate(key);
            return;
        }
        if (key === "playerActivity")
            return;
        if (key === "back") {
            if (keyboard.opened) {
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
        focusables(keyboard.opened ? keyboard : scene, list);
        if (!current) {
            if (list.length)
                list[0].forceActiveFocus();
            return;
        }
        let c = current.mapToItem(scene, current.width / 2, current.height / 2), best = null, score = 1e12;
        for (let item of list) {
            if (item === current)
                continue;
            if ((key === "up" || key === "down") && ((current.objectName || "").indexOf("nav_") === 0) !== ((item.objectName || "").indexOf("nav_") === 0))
                continue;
            let p = item.mapToItem(scene, item.width / 2, item.height / 2), dx = p.x - c.x, dy = p.y - c.y;
            let along = key === "left" ? -dx : key === "right" ? dx : key === "up" ? -dy : dy;
            let across = key === "left" || key === "right" ? Math.abs(dy) : Math.abs(dx);
            if (along < 8)
                continue;
            // Prefer the nearest overlapping row/column, even when controls have
            // different widths (compact launchers above narrow poster cards).
            let span = key === "left" || key === "right" ? (item.height + current.height) / 2 : (item.width + current.width) / 2;
            let gap = Math.max(0, across - span);
            let s = along + gap * 3 + gap * gap / Math.max(40, along) + across * 0.1;
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
            if (keyboard.opened)
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
        keyboard.opened = false;
    }
    Motion {
        id: pageMotion
        shown: true
        reducedMotion: reduced
        duration: 280
    }
    onPageChanged: pageMotion.replay()
    onReducedChanged: if (reduced)
        backdropFade.complete()
    onBackdropChanged: {
        if (backdrop !== newBackdrop) {
            oldBackdrop = inLibrary && backdrop ? newBackdrop : "";
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
            if (playerShowing) {
                playbackView.focusControl();
                return;
            }
            if (page === "home")
                firstFocus(launchers);
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
    Loader {
        anchors.fill: parent
        active: !!app.nativePlayer
        source: active ? "NativeVideo.qml" : ""
        onLoaded: item.ready.connect(success => send({
                type: "videoReady",
                ready: success
            }))
        onStatusChanged: if (status === Loader.Error)
            send({
                type: "videoReady",
                ready: false
            })
    }
    Player {
        id: playbackView
        objectName: "playbackView"
        anchors.fill: parent
        visible: playerShowing
        playback: app.playback || ({})
        title: app.playerTitle || ""
        subtitle: app.playerSubtitle || ""
        nextEpisode: app.nextEpisode || ({})
        switchingEpisode: !!app.switchingEpisode
        notice: lib.message || ""
        accent: window.accent
        reducedMotion: reduced
        onAction: value => send(value)
    }
    Item {
        id: scene
        visible: !playerShowing
        opacity: reduced ? 1 : sessionMotion.progress
        transform: Translate {
            y: reduced ? 0 : 12 * (1 - sessionMotion.progress)
        }
        width: 1920
        height: 1080
        anchors.centerIn: parent
        scale: Math.min(window.width / width, window.height / height)
        Ambient {
            anchors.fill: parent
        }
        Image {
            id: previousImage
            visible: inLibrary && (page === "home" || page === "detail") && !!oldBackdrop && !!newBackdrop
            anchors.fill: parent
            source: oldBackdrop
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            opacity: newBackdrop ? (page === "home" ? 0.8 : 0.85) : 0
            sourceSize.width: 1920
        }
        Image {
            id: newImage
            objectName: "libraryBackdrop"
            visible: inLibrary && (page === "home" || page === "detail") && !!newBackdrop
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
            to: newBackdrop ? (page === "home" ? 0.8 : 0.85) : 0
            duration: reduced ? 0 : 600
            onDurationChanged: if (duration === 0)
                complete()
            easing.type: Easing.OutCubic
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: "#ff141414"
                }
                GradientStop {
                    position: .35
                    color: "#c7141414"
                }
                GradientStop {
                    position: 1
                    color: "#18141414"
                }
            }
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: "#60141414"
                }
                GradientStop {
                    position: .4
                    color: "#00141414"
                }
                GradientStop {
                    position: .85
                    color: "#f5141414"
                }
                GradientStop {
                    position: 1
                    color: "#ff141414"
                }
            }
        }
        // A soft vignette leaves every focus target readable over artwork.
        Rectangle {
            anchors.fill: parent
            color: "#141414"
            opacity: page === "detail" ? Math.min(0.5, scroll.contentY / 900) : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: reduced ? 0 : 180
                    onDurationChanged: if (duration === 0)
                        complete()
                }
            }
        }
        Item {
            id: header
            x: 168
            y: 55
            width: 1664
            height: 66
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "TV MODE"
                visible: !rail.expanded
                font.family: "Inter"
                font.letterSpacing: 3
                font.pixelSize: 22
                font.weight: Font.DemiBold
                color: "#f5f5f5"
            }
            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 28
                TvButton {
                    objectName: "exitTv"
                    text: "Exit TV"
                    iconName: "exit"
                    quiet: true
                    width: 174
                    height: 56
                    accent: window.accent
                    reducedMotion: reduced
                    onClicked: send({
                        type: "exit"
                    })
                }
                Text {
                    text: lib.account || ""
                    visible: inLibrary && !!lib.account
                    font.family: "Inter"
                    font.pixelSize: 21
                    color: "#b3b3b3"
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    text: clockText
                    font.family: "Inter"
                    font.pixelSize: 25
                    color: "#eeeeee"
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
        Rectangle {
            x: 0
            y: 0
            width: 340
            height: 1080
            z: 4
            opacity: Math.min(1, railMotion.progress * 1.5)
            visible: opacity > 0
            transform: Translate {
                x: -12 * (1 - railMotion.progress)
            }
            Text {
                x: 35
                y: 76
                text: "TV MODE"
                font.family: "Inter"
                font.pixelSize: 22
                font.letterSpacing: 3
                color: "white"
            }
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: "#ff141414"
                }
                GradientStop {
                    position: 0.8
                    color: "#fa141414"
                }
                GradientStop {
                    position: 1
                    color: "#00141414"
                }
            }
        }
        Motion {
            id: railMotion
            shown: rail.expanded
            reducedMotion: reduced
        }
        Column {
            id: rail
            objectName: "rail"
            x: 35
            y: 278
            spacing: 22
            z: 5
            readonly property bool expanded: !!window.activeFocusItem && (window.activeFocusItem.objectName || "").indexOf("nav_") === 0
            Repeater {
                model: [
                    {
                        name: "Home",
                        route: "home"
                    },
                    {
                        name: "Search",
                        route: "search"
                    },
                    {
                        name: "Movies",
                        route: "movie"
                    },
                    {
                        name: "Series",
                        route: "series"
                    },
                    {
                        name: "Watchlist",
                        route: "saved"
                    },
                    {
                        name: "Settings",
                        route: "account"
                    }
                ]
                TvButton {
                    required property var modelData
                    objectName: "nav_" + modelData.route
                    labelAreaWidth: 248
                    text: modelData.name
                    labelOpacity: railMotion.progress
                    surfaceWidth: 72 + 176 * railMotion.progress
                    width: rail.expanded ? 248 : 72
                    height: 72
                    iconName: modelData.route
                    quiet: true
                    accent: window.accent
                    reducedMotion: reduced
                    selected: page === modelData.route || (page === "browse" && (modelData.route === "saved" ? lib.browseList === "watchlist" : lib.browseList !== "watchlist" && lib.browseKind === modelData.route))
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
                        anchors.top: parent.bottom
                        anchors.topMargin: 4
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: modelData.name
                        color: "#ffffff"
                        font.pixelSize: 16
                        visible: false
                    }
                }
            }
        }
        MediaGrid {
            id: catalogue
            opacity: pageMotion.progress
            transform: Translate {
                y: 16 * (1 - pageMotion.progress)
            }
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
            interactive: !keyboard.opened
            contentY: scrollTarget
            Behavior on contentY {
                NumberAnimation {
                    duration: reduced ? 0 : 280
                    onDurationChanged: if (duration === 0)
                        complete()
                    easing.type: Easing.OutCubic
                }
            }
            Item {
                id: content
                opacity: pageMotion.progress
                transform: Translate {
                    y: 16 * (1 - pageMotion.progress)
                }
                x: 18
                width: 1664
                height: page === "login" ? 850 : page === "search" ? 770 : page === "account" ? 830 : page === "detail" ? detailArea.height + (episodeSection.visible ? episodeSection.height : 0) : shelfColumn.y + shelfColumn.height
                Item {
                    id: homeFeature
                    objectName: "homeFeature"
                    visible: page === "home"
                    width: 1080
                    height: inLibrary ? featuredMore.y + featuredMore.height : 140
                    Text {
                        id: featuredTitle
                        y: 32
                        width: 1040
                        text: inLibrary ? (hero.title || "Home") : "Home"
                        color: "white"
                        font.family: "Inter"
                        font.pixelSize: 64
                        font.weight: Font.DemiBold
                        font.letterSpacing: -1
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                    Text {
                        id: featuredMetadata
                        y: featuredTitle.y + featuredTitle.height + 16
                        width: 1000
                        text: hero.subtitle || ""
                        visible: inLibrary
                        font.family: "Inter"
                        font.pixelSize: 23
                        color: "#cccccc"
                        elide: Text.ElideRight
                    }
                    Text {
                        id: featuredSynopsis
                        y: featuredMetadata.y + featuredMetadata.height + 18
                        width: 920
                        text: hero.overview || ""
                        visible: inLibrary
                        font.family: "Inter"
                        font.pixelSize: 26
                        color: "#dddddd"
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                    TvButton {
                        id: featuredMore
                        objectName: "featuredDetails"
                        y: Math.max(266, featuredSynopsis.y + featuredSynopsis.height + 24)
                        text: "More info"
                        iconName: "info"
                        primary: true
                        height: 60
                        visible: inLibrary && !!hero.id
                        onClicked: openTitle(hero)
                        reducedMotion: reduced
                    }
                }
                Row {
                    id: launchers
                    y: inLibrary ? homeFeature.height + 44 : 190
                    spacing: 20
                    visible: page === "home"
                    Repeater {
                        model: [
                            {
                                name: "Oblecto",
                                icon: "oblecto",
                                artwork: "",
                                tint: "#202d32",
                                route: "open"
                            },
                            {
                                name: "Steam",
                                icon: "gamepad",
                                artwork: "file:///usr/share/icons/hicolor/256x256/apps/steam.png",
                                tint: "#202735",
                                route: "steam"
                            },
                            {
                                name: "Kodi",
                                icon: "media",
                                artwork: "file:///usr/share/icons/hicolor/scalable/apps/kodi.svg",
                                tint: "#222c32",
                                route: "kodi"
                            }
                        ]
                        AppTile {
                            required property var modelData
                            objectName: "launcher_" + modelData.route
                            title: modelData.name
                            iconName: modelData.icon
                            artwork: modelData.artwork
                            tint: modelData.tint
                            accent: window.accent
                            reducedMotion: reduced
                            onClicked: modelData.route === "open" ? send(inLibrary ? {
                                type: "browse",
                                kind: "movie",
                                list: "all"
                            } : {
                                type: "open"
                            }) : send({
                                type: "app",
                                name: modelData.route
                            })
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
                        font.letterSpacing: -1
                        color: "#ffffff"
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
                                quiet: true
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
                    height: Math.max(590, detail.kind === "series" ? detailActions.y + detailActions.height + 50 : detailSources.y + detailSources.height + 50)
                    visible: page === "detail" && !lib.loading
                    Image {
                        x: 1280
                        y: 85
                        width: 300
                        height: 450
                        source: (lib.assets || {})[detail.art || ""] || ""
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        opacity: status === Image.Ready ? 0.9 * Math.max(0, 1 - scroll.contentY / 260) : 0
                        Behavior on opacity {
                            NumberAnimation {
                                duration: reduced ? 0 : 320
                                onDurationChanged: if (duration === 0)
                                    complete()
                            }
                        }
                    }
                    Text {
                        id: detailTitle
                        y: 48
                        width: 1130
                        text: detail.title || ""
                        font.family: "Inter"
                        font.pixelSize: 68
                        font.weight: Font.DemiBold
                        font.letterSpacing: -1
                        color: "#ffffff"
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                    Text {
                        id: detailMetadata
                        y: detailTitle.y + detailTitle.height + 24
                        text: detail.subtitle || ""
                        font.family: "Inter"
                        font.pixelSize: 23
                        color: "#cccccc"
                    }
                    Text {
                        id: detailOverview
                        y: detailMetadata.y + detailMetadata.height + 24
                        width: 1090
                        text: detail.overview || "No synopsis available."
                        font.family: "Inter"
                        font.pixelSize: 27
                        lineHeight: 1.3
                        color: "#cccccc"
                        wrapMode: Text.Wrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                    }
                    Row {
                        id: detailActions
                        y: detailOverview.y + detailOverview.height + 40
                        spacing: 17
                        TvButton {
                            text: detail.resumable ? "Resume · " + detail.resume : "Play"
                            iconName: "play"
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
                            objectName: "browseEpisodes"
                            text: "Episodes"
                            iconName: "series"
                            primary: true
                            visible: detail.kind === "series" && shelves.length > 0
                            reducedMotion: reduced
                            onClicked: firstFocus(episodeSection)
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
                            text: detail.favourite ? "Favourited" : "Favourite"
                            iconName: "heart"
                            quiet: true
                            onClicked: send({
                                type: "flag",
                                flag: "favourite",
                                enabled: !detail.favourite
                            })
                            reducedMotion: reduced
                        }
                        TvButton {
                            text: detail.watchlist ? "In watchlist" : "Watchlist"
                            iconName: detail.watchlist ? "check" : "plus"
                            quiet: true
                            visible: detail.kind !== "episode"
                            onClicked: send({
                                type: "flag",
                                flag: "watchlist",
                                enabled: !detail.watchlist
                            })
                            reducedMotion: reduced
                        }
                        TvButton {
                            text: detail.watched ? "Watched" : "Mark watched"
                            quiet: true
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
                        id: detailSources
                        y: detailActions.y + detailActions.height + 36
                        spacing: 20
                        visible: detail.kind !== "series"
                        TvButton {
                            id: sourcePicker
                            objectName: "sourcePicker"
                            property int currentIndex: 0
                            property var sources: detail.files || []
                            onSourcesChanged: currentIndex = 0
                            width: 890
                            quiet: true
                            flat: true
                            text: "Version " + (currentIndex + 1) + " of " + sources.length + "  ·  " + (sources[currentIndex] ? sources[currentIndex].label : "")
                            visible: sources.length > 1
                            onClicked: currentIndex = (currentIndex + 1) % sources.length
                            reducedMotion: reduced
                        }
                        Text {
                            text: "No playable source is linked to this title."
                            font.pixelSize: 25
                            color: "#b3b3b3"
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
                    Text {
                        visible: !lib.loading && shelves.length === 0
                        text: "No episodes available."
                        font.family: "Inter"
                        font.pixelSize: 25
                        color: "#b3b3b3"
                    }
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
                                quiet: true
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
                    y: inLibrary ? launchers.y + launchers.height + 26 : 360
                    width: parent.width
                    spacing: 28
                    visible: page === "home"
                    Repeater {
                        model: page === "home" && inLibrary ? shelves : []
                        Column {
                            id: shelf
                            required property var modelData
                            required property int index
                            width: shelfColumn.width
                            spacing: 16
                            Item {
                                width: parent.width
                                height: 50
                                Text {
                                    text: shelf.modelData.title
                                    anchors.verticalCenter: parent.verticalCenter
                                    font.family: "Inter"
                                    font.pixelSize: 29
                                    font.weight: Font.DemiBold
                                    color: "#eeeeee"
                                }
                                TvButton {
                                    anchors.right: parent.right
                                    width: 240
                                    height: 50
                                    text: "View all"
                                    quiet: true
                                    visible: shelf.modelData.title === "Movies" || shelf.modelData.title === "Series"
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
                                        onDurationChanged: if (duration === 0)
                                            complete()
                                        easing.type: Easing.OutCubic
                                    }
                                }
                            }
                        }
                    }
                    Text {
                        visible: !lib.loading && inLibrary && shelves.length === 0 && page !== "detail"
                        text: "Your library is empty."
                        font.family: "Inter"
                        font.pixelSize: 27
                        color: "#b3b3b3"
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
                        text: profiles.length ? "Choose a profile" : "Sign in"
                        font.family: "Inter"
                        font.pixelSize: 56
                        font.weight: Font.DemiBold
                        font.letterSpacing: -1
                        color: "#ffffff"
                    }
                    Row {
                        id: profileRow
                        y: 175
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
                                    width: 140
                                    height: 140
                                    radius: 4
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    color: ["#39485b", "#71514c", "#4b6352", "#655472"][index % 4]
                                    border.width: profile.activeFocus ? 4 : 0
                                    border.color: "#ffffff"
                                    scale: profile.activeFocus ? 1.03 : 1
                                    Behavior on scale {
                                        NumberAnimation {
                                            duration: reduced ? 0 : 180
                                            onDurationChanged: if (duration === 0)
                                                complete()
                                        }
                                    }
                                    Text {
                                        anchors.centerIn: parent
                                        text: (profile.modelData.name || profile.modelData.username || "?").substring(0, 1).toUpperCase()
                                        font.family: "Inter"
                                        font.pixelSize: 47
                                        font.weight: Font.Medium
                                        color: "white"
                                    }
                                }
                                Text {
                                    y: 154
                                    width: parent.width
                                    text: modelData.name || modelData.username
                                    horizontalAlignment: Text.AlignHCenter
                                    font.family: "Inter"
                                    font.pixelSize: 25
                                    color: profile.activeFocus ? "#ffffff" : "#b3b3b3"
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
                        y: profiles.length ? 390 : 190
                        spacing: 20
                        Column {
                            spacing: 12
                            Text {
                                text: "Username"
                                font.pixelSize: 22
                                color: "#b3b3b3"
                            }
                            TextField {
                                id: username
                                objectName: "username"
                                width: 440
                                height: 72
                                placeholderTextColor: "#999999"
                                placeholderText: "Username"
                                font.pixelSize: 26
                                font.family: "Inter"
                                color: "#ffffff"
                                leftPadding: 22
                                activeFocusOnTab: true
                                Accessible.name: "Username"
                                function openKeyboard() {
                                    window.openKeyboard(username);
                                }
                                Keys.onReturnPressed: openKeyboard()
                                background: Rectangle {
                                    radius: 3
                                    color: "#252525"
                                    border.width: username.activeFocus ? 3 : 1
                                    border.color: username.activeFocus ? "white" : "#555555"
                                }
                            }
                        }
                        Column {
                            spacing: 12
                            Text {
                                text: "Password"
                                font.pixelSize: 22
                                color: "#b3b3b3"
                            }
                            TextField {
                                id: password
                                objectName: "password"
                                width: 440
                                height: 72
                                placeholderTextColor: "#999999"
                                placeholderText: "Password"
                                echoMode: TextInput.Password
                                font.pixelSize: 26
                                font.family: "Inter"
                                color: "#ffffff"
                                leftPadding: 22
                                activeFocusOnTab: true
                                Accessible.name: "Password"
                                function openKeyboard() {
                                    window.openKeyboard(password);
                                }
                                onAccepted: window.login()
                                background: Rectangle {
                                    radius: 3
                                    color: "#252525"
                                    border.width: password.activeFocus ? 3 : 1
                                    border.color: password.activeFocus ? "white" : "#555555"
                                }
                            }
                        }
                        TvButton {
                            text: lib.loading ? "Connecting…" : "Sign in"
                            y: 38
                            height: 72
                            primary: true
                            accent: window.accent
                            enabled: !lib.loading
                            onClicked: window.login()
                            reducedMotion: reduced
                        }
                    }
                    Row {
                        y: profiles.length ? 530 : 330
                        spacing: 28
                        CheckBox {
                            id: remember
                            objectName: "remember"
                            text: "Remember me"
                            checked: true
                            font.family: "Inter"
                            font.pixelSize: 23
                            activeFocusOnTab: true
                            contentItem: Text {
                                text: remember.text
                                color: "#cccccc"
                                font: remember.font
                                leftPadding: remember.indicator.width + remember.spacing
                                verticalAlignment: Text.AlignVCenter
                            }
                            indicator: Rectangle {
                                width: 28
                                height: 28
                                y: (remember.height - height) / 2
                                color: remember.checked ? "white" : "transparent"
                                border.width: remember.activeFocus ? 3 : 1
                                border.color: "white"
                                radius: 2
                                Icon {
                                    anchors.centerIn: parent
                                    width: 22
                                    height: 22
                                    name: "check"
                                    color: "#141414"
                                    visible: remember.checked
                                }
                            }
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
                        y: profiles.length ? 640 : 440
                        spacing: 20
                        visible: serverSettings
                        TextField {
                            id: server
                            width: 900
                            height: 70
                            text: lib.server || ""
                            font.pixelSize: 23
                            color: "#ffffff"
                            leftPadding: 20
                            activeFocusOnTab: true
                            function openKeyboard() {
                                window.openKeyboard(server);
                            }
                            Keys.onReturnPressed: openKeyboard()
                            background: Rectangle {
                                radius: 3
                                color: "#252525"
                                border.width: server.activeFocus ? 3 : 1
                                border.color: "white"
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
                        text: "Search"
                        font.family: "Inter"
                        font.pixelSize: 56
                        font.weight: Font.DemiBold
                        font.letterSpacing: -1
                        color: "#ffffff"
                    }
                    TextField {
                        id: searchField
                        objectName: "searchField"
                        y: 170
                        width: 1250
                        height: 86
                        placeholderTextColor: "#999999"
                        placeholderText: "Titles, movies, series"
                        font.pixelSize: 32
                        font.family: "Inter"
                        color: "#ffffff"
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
                            radius: 3
                            color: "#252525"
                            border.width: searchField.activeFocus ? 3 : 1
                            border.color: "white"
                        }
                    }
                    Row {
                        y: 295
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
                Item {
                    id: accountArea
                    visible: page === "account"
                    width: parent.width
                    height: 830
                    Text {
                        y: 50
                        text: "Settings"
                        font.family: "Inter"
                        font.pixelSize: 56
                        font.weight: Font.DemiBold
                        color: "white"
                        font.letterSpacing: -1
                    }
                    Text {
                        y: 144
                        width: 440
                        text: lib.account || "Guest"
                        font.family: "Inter"
                        font.pixelSize: 30
                        color: "white"
                        elide: Text.ElideRight
                    }
                    Text {
                        y: 198
                        width: 440
                        text: lib.server || ""
                        font.family: "Inter"
                        font.pixelSize: 22
                        color: "#999999"
                        wrapMode: Text.WrapAnywhere
                    }
                    Text {
                        y: 310
                        width: 440
                        text: "Playback preferences are managed in Oblecto."
                        font.family: "Inter"
                        font.pixelSize: 23
                        color: "#b3b3b3"
                        wrapMode: Text.Wrap
                    }
                    Text {
                        y: 750
                        width: 440
                        text: app.controller || "Keyboard & remote"
                        font.family: "Inter"
                        font.pixelSize: 20
                        color: "#999999"
                        elide: Text.ElideRight
                    }
                    Column {
                        x: 520
                        y: 50
                        width: 1100
                        spacing: 8
                        Text {
                            text: "Account & display"
                            height: 46
                            font.family: "Inter"
                            font.pixelSize: 26
                            color: "#b3b3b3"
                        }
                        TvButton {
                            text: "Refresh library"
                            iconName: "refresh"
                            flat: true
                            width: 1100
                            height: 76
                            reducedMotion: reduced
                            onClicked: send({
                                type: "home"
                            })
                        }
                        TvButton {
                            text: inLibrary ? "Sign out" : "Sign in"
                            iconName: "profile"
                            valueText: lib.account || "Guest"
                            flat: true
                            width: 1100
                            height: 76
                            reducedMotion: reduced
                            onClicked: send({
                                type: inLibrary ? "logout" : "open"
                            })
                        }
                        TvButton {
                            text: "Motion"
                            iconName: "motion"
                            valueText: reduced ? "Reduced" : "Full"
                            flat: true
                            width: 1100
                            height: 76
                            reducedMotion: reduced
                            onClicked: send({
                                type: "motion",
                                reduced: !reduced
                            })
                        }
                        Item {
                            width: 1
                            height: 22
                        }
                        Text {
                            text: "System"
                            height: 46
                            font.family: "Inter"
                            font.pixelSize: 26
                            color: "#b3b3b3"
                        }
                        TvButton {
                            text: "Sleep"
                            iconName: "sleep"
                            valueText: "Hold 2 seconds"
                            flat: true
                            width: 1100
                            height: 76
                            hold: true
                            reducedMotion: reduced
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
                            text: "Shut down"
                            iconName: "power"
                            valueText: "Hold 2 seconds"
                            flat: true
                            width: 1100
                            height: 76
                            hold: true
                            reducedMotion: reduced
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
                            iconName: "exit"
                            flat: true
                            width: 1100
                            height: 76
                            reducedMotion: reduced
                            onClicked: send({
                                type: "exit"
                            })
                        }
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
                    color: "#00141414"
                }
                GradientStop {
                    position: .35
                    color: "#ff141414"
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
                        key: "Guide",
                        label: "Home (hold)"
                    },
                    {
                        key: "View",
                        label: "Exit TV"
                    }
                ]
                Row {
                    required property var modelData
                    spacing: 9
                    Rectangle {
                        width: Math.max(27, hintKey.implicitWidth + 14)
                        height: 27
                        radius: 6
                        color: "#2b3038"
                        Text {
                            id: hintKey
                            anchors.centerIn: parent
                            text: modelData.key
                            font.pixelSize: 16
                            font.weight: Font.DemiBold
                            color: "#dddddd"
                        }
                    }
                    Text {
                        text: modelData.label
                        font.family: "Inter"
                        font.pixelSize: 18
                        color: "#999999"
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
            text: "Preview"
            font.family: "Inter"
            font.pixelSize: 15
            color: "#777777"
            visible: !!app.preview && !app.playing && !app.activeApp
        }
        Rectangle {
            id: toast
            property string dismissedMessage: ""
            property string displayedMessage: ""
            property string message: lib.message || ""
            onMessageChanged: {
                dismissedMessage = "";
                if (message)
                    displayedMessage = message;
            }
            Motion {
                id: toastMotion
                shown: !!toast.message && toast.message !== toast.dismissedMessage
                reducedMotion: reduced
            }
            opacity: toastMotion.progress
            transform: Translate {
                y: 12 * (1 - toastMotion.progress)
            }
            enabled: toastMotion.shown
            x: 260
            y: 920
            width: 1400
            height: 74
            radius: 4
            color: "#f22b2b2b"
            border.color: "#555555"
            visible: opacity > 0
            z: 9
            Text {
                anchors.fill: parent
                anchors.margins: 20
                text: toast.displayedMessage
                font.family: "Inter"
                font.pixelSize: 23
                color: "#ffffff"
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
            MouseArea {
                anchors.fill: parent
                onClicked: toast.dismissedMessage = toast.message
            }
        }
        Rectangle {
            x: 168
            y: 132
            height: 3
            width: 1664
            color: "#e50914"
            opacity: lib.loading ? 0.8 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: reduced ? 0 : 180
                    onDurationChanged: if (duration === 0)
                        complete()
                }
            }
        }
        Rectangle {
            anchors.fill: parent
            color: "#b0000000"
            opacity: keyboard.reveal
            visible: opacity > 0
            enabled: keyboard.opened
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
            y: 590
            z: 20
            accent: window.accent
            reducedMotion: reduced
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
