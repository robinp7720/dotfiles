import QtQuick

Image {
    id: icon
    property string name: ""
    property color color: "#e8eaed"
    readonly property var paths: ({
            home: '<path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1z"/>',
            search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/>',
            movie: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M7 3v18M17 3v18M3 8h4M3 16h4M17 8h4M17 16h4"/>',
            series: '<rect x="2" y="5" width="20" height="14" rx="2"/><path d="m8 2 4 3 4-3M8 22h8"/>',
            saved: '<path d="M6 3h12v18l-6-4-6 4z"/>',
            account: '<path d="m9 3-.5 3-2 .9-2.7-1-2.5 4.2 2.3 1.9v2l-2.3 1.9 2.5 4.2 2.7-1 2 .9.5 3h5l.5-3 2-.9 2.7 1 2.5-4.2-2.3-1.9v-2l2.3-1.9-2.5-4.2-2.7 1-2-.9-.5-3z" transform="translate(1 -1) scale(.92)"/><circle cx="12" cy="12" r="3"/>',
            exit: '<path d="M9 4H4v16h5M10 12h11m-5-5 5 5-5 5"/>',
            play: '<path d="m8 4 12 8-12 8z" fill="currentColor" stroke="none"/>',
            pause: '<path d="M7 4h3v16H7zM15 4h3v16h-3z" fill="currentColor" stroke="none"/>',
            back: '<path d="m10 4-8 8 8 8M2 12h20"/>',
            close: '<path d="m5 5 14 14M5 19 19 5"/>',
            check: '<path d="m4 12 5 5L20 6"/>',
            next: '<path d="m5 5 10 7-10 7z" fill="currentColor" stroke="none"/><path d="M19 5v14"/>',
            chapter: '<path d="m4 5 9 7-9 7z"/><path d="M17 5v14M21 5v14"/>',
            rewind: '<path d="M5 7a9 9 0 1 1-2 10M5 2v5H0"/><text x="6" y="16" font-family="sans-serif" font-size="9" font-weight="bold" fill="currentColor" stroke="none">10</text>',
            forward: '<path d="M19 7a9 9 0 1 0 2 10M19 2v5h5"/><text x="6" y="16" font-family="sans-serif" font-size="9" font-weight="bold" fill="currentColor" stroke="none">10</text>',
            volumeDown: '<path d="M3 9h4l5-4v14l-5-4H3zM17 12h5"/>',
            volumeUp: '<path d="M3 9h4l5-4v14l-5-4H3zM17 12h5M19.5 9.5v5"/>',
            audio: '<path d="M4 13v-1a8 8 0 0 1 16 0v1M4 12H2v8h5v-8zM20 12h2v8h-5v-8z"/>',
            subtitles: '<path d="M3 4h18v14H9l-5 4v-4H3zM7 9h4M14 9h3M7 13h3M13 13h4"/>',
            quality: '<circle cx="12" cy="12" r="7"/><circle cx="12" cy="12" r="2.5"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"/>',
            oblecto: '<circle cx="12" cy="12" r="9"/><path d="m10 8 6 4-6 4z" fill="currentColor" stroke="none"/>',
            gamepad: '<path d="M7 7h10c3 0 4 3 5 10 .4 3-2 4-4 1l-2-2H8l-2 2c-2 3-4.4 2-4-1 1-7 2-10 5-10zM5 11h6M8 8v6"/><circle cx="16" cy="11" r=".8"/><circle cx="19" cy="13" r=".8"/>',
            media: '<path d="m12 2 10 10-10 10L2 12zM9 8l6 4-6 4z"/>'
        })
    source: "data:image/svg+xml," + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" color="' + color + '" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">' + (paths[name] || "") + '</svg>')
    sourceSize.width: width * 2
    sourceSize.height: height * 2
    fillMode: Image.PreserveAspectFit
    smooth: true
}
