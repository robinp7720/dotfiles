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
