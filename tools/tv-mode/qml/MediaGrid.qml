import QtQuick
import QtQuick.Controls

Item {
    id: browser
    property var items: []
    property var assets: ({})
    property color accent: "#83d2e3"
    property bool reducedMotion: false
    property bool loading: false
    property bool moreLoading: false
    property string cursor: ""
    readonly property bool hasMore: cursor.length > 0
    property string error: ""
    property int routeSerial: 0
    property int restoreIndex: 0
    property real restoreScroll: 0
    property int columns: 7
    property int pendingIndex: -1
    property bool requested: false
    property bool rebuilding: false
    property bool ready: false
    property int lastSerial: -1
    property alias currentIndex: grid.currentIndex
    property alias contentY: grid.contentY
    readonly property int count: titles.count
    signal chosen(var media)
    signal highlighted(var media)
    signal loadMore
    signal positionSaved(int index, real scroll)
    signal artworkRequested(var paths)

    function sync() {
        if (!ready)
            return;
        let changedRoute = lastSerial !== routeSerial;
        let samePrefix = !changedRoute && items.length >= titles.count;
        if (samePrefix)
            for (let i = 0; i < titles.count; i++) {
                let old = JSON.parse(titles.get(i).payload);
                if (old.id !== items[i].id || old.kind !== items[i].kind) {
                    samePrefix = false;
                    break;
                }
            }
        rebuilding = true;
        if (!samePrefix) {
            titles.clear();
            pendingIndex = -1;
        }
        for (let i = 0; i < items.length; i++) {
            let payload = JSON.stringify(items[i]);
            if (i >= titles.count)
                titles.append({
                    payload: payload
                });
            else if (titles.get(i).payload !== payload)
                titles.setProperty(i, "payload", payload);
        }
        lastSerial = routeSerial;
        if (changedRoute) {
            grid.currentIndex = Math.min(restoreIndex, Math.max(0, titles.count - 1));
            grid.contentY = restoreScroll;
            requested = false;
        }
        rebuilding = false;
        if (pendingIndex >= 0 && pendingIndex < titles.count)
            focusAt(pendingIndex);
        viewportTimer.restart();
    }
    function focusAt(index) {
        if (!titles.count)
            return;
        grid.currentIndex = Math.max(0, Math.min(index, titles.count - 1));
        grid.positionViewAtIndex(grid.currentIndex, GridView.Contain);
        pendingIndex = -1;
        focusTimer.restart();
    }
    function restoreFocus() {
        if (!titles.count)
            return;
        grid.currentIndex = Math.min(restoreIndex, titles.count - 1);
        grid.contentY = Math.max(0, Math.min(restoreScroll, grid.contentHeight - grid.height));
        focusTimer.restart();
    }
    function fetchMore() {
        if (hasMore && !loading && !moreLoading && !requested && !error) {
            requested = true;
            loadMore();
        }
    }
    function navigate(key) {
        let index = grid.currentIndex, col = index % columns;
        if (key === "up" && index < columns)
            return false;
        if (key === "left" && col === 0)
            return false;
        if (key === "right" && col === columns - 1)
            return true;
        let next = index + (key === "up" ? -columns : key === "down" ? columns : key === "left" ? -1 : 1);
        if (next >= titles.count) {
            if (hasMore) {
                pendingIndex = next;
                fetchMore();
            } else if (key === "down" && Math.floor(index / columns) < Math.floor((titles.count - 1) / columns))
                focusAt(titles.count - 1);
            else if (error)
                retry.forceActiveFocus();
            return true;
        }
        focusAt(next);
        return true;
    }
    function viewport() {
        if (!visible || rebuilding)
            return;
        let start = Math.max(0, Math.floor(grid.contentY / grid.cellHeight) * columns);
        let end = Math.min(titles.count, start + (Math.ceil(grid.height / grid.cellHeight) + 1) * columns);
        let paths = [];
        for (let i = start; i < end; i++) {
            let m = JSON.parse(titles.get(i).payload);
            if (m.art)
                paths.push(m.art);
        }
        if (paths.length)
            artworkRequested(paths);
        if (titles.count && (end >= titles.count || grid.currentIndex >= titles.count - columns * 2))
            fetchMore();
        positionSaved(Math.max(0, grid.currentIndex), Math.max(0, grid.contentY));
    }
    onItemsChanged: sync()
    onRouteSerialChanged: sync()
    onMoreLoadingChanged: {
        if (!moreLoading)
            requested = false;
        viewportTimer.restart();
    }
    onLoadingChanged: viewportTimer.restart()
    onCursorChanged: {
        requested = false;
        viewportTimer.restart();
    }
    onVisibleChanged: if (visible)
        viewportTimer.restart()
    Component.onCompleted: {
        ready = true;
        sync();
    }
    ListModel {
        id: titles
    }
    Timer {
        id: focusTimer
        interval: 16
        onTriggered: {
            let item = grid.itemAtIndex(grid.currentIndex);
            if (item)
                item.focusCard();
            else if (browser.visible)
                restart();
        }
    }
    Timer {
        id: viewportTimer
        interval: 120
        onTriggered: browser.viewport()
    }
    GridView {
        id: grid
        objectName: "catalogueGrid"
        anchors.fill: parent
        anchors.bottomMargin: 42
        model: titles
        cellWidth: width / browser.columns
        cellHeight: 334
        clip: true
        cacheBuffer: cellHeight
        keyNavigationEnabled: false
        boundsBehavior: Flickable.StopAtBounds
        snapMode: GridView.SnapToRow
        onContentYChanged: if (!browser.rebuilding)
            viewportTimer.restart()
        ScrollBar.vertical: ScrollBar {
            policy: ScrollBar.AsNeeded
            active: true
            width: 7
        }
        delegate: Item {
            required property int index
            required property string payload
            property var gridOwner: browser
            property var media: JSON.parse(payload)
            signal chosen(var media)
            onChosen: media => browser.chosen(media)
            width: grid.cellWidth
            height: grid.cellHeight
            function focusCard() {
                card.forceActiveFocus();
            }
            Poster {
                id: card
                property var gridOwner: browser
                x: 12
                y: 9
                width: 190
                height: 322
                artHeight: 266
                compact: true
                media: JSON.parse(payload)
                assets: browser.assets
                accent: browser.accent
                reducedMotion: browser.reducedMotion
                onChosen: media => {
                    browser.positionSaved(index, grid.contentY);
                    browser.chosen(media);
                }
                onHighlighted: media => {
                    grid.currentIndex = index;
                    browser.highlighted(media);
                    viewportTimer.restart();
                }
            }
        }
    }
    Text {
        anchors.centerIn: parent
        width: 900
        horizontalAlignment: Text.AlignHCenter
        visible: !browser.loading && !titles.count
        text: "No titles here yet. Try another filter or search."
        font.family: "Inter"
        font.pixelSize: 27
        color: "#aec2cd"
        wrapMode: Text.Wrap
    }
    Text {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.leftMargin: 12
        text: browser.loading || browser.moreLoading ? "Loading titles…" : browser.error ? "Couldn't load more titles. Your place is saved." : browser.hasMore ? titles.count + " titles · Keep exploring" : titles.count + " titles · All caught up"
        font.family: "Inter"
        font.pixelSize: 18
        color: "#9ab1bf"
    }
    TvButton {
        id: retry
        objectName: "retryMore"
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        text: "Retry"
        height: 40
        width: 150
        visible: !!browser.error
        onClicked: {
            browser.requested = true;
            browser.loadMore();
        }
        accent: browser.accent
        reducedMotion: browser.reducedMotion
    }
}
