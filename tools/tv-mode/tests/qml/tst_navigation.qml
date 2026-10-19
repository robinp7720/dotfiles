import QtQuick
import QtTest
import "../../qml" as TV

TestCase {
    id: test
    name: "TVNavigation"
    when: windowShown
    property var actions: []
    property int serial: 1
    QtObject {
        id: fake
        property var state: ({
                library: {
                    page: "home",
                    serial: 0,
                    rows: [],
                    assets: {},
                    profiles: [],
                    authenticated: false
                },
                theme: {
                    primary: "#83d2e3"
                },
                reducedMotion: true
            })
        signal input(string key)
        function send(a) {
            test.actions.push(a);
        }
    }
    TV.Main {
        id: shell
        bridge: fake
        visible: true
    }
    function route(page, extra) {
        let lib = {
            page: page,
            serial: serial++,
            rows: [],
            assets: {},
            profiles: [],
            authenticated: true,
            server: "https://example.test",
            loading: false
        };
        if (extra)
            for (let key in extra)
                lib[key] = extra[key];
        fake.state = {
            library: lib,
            theme: {
                primary: "#83d2e3"
            },
            reducedMotion: true
        };
        shell.requestActivate();
        wait(140);
    }
    function init() {
        actions = [];
        route("home", {
            authenticated: false
        });
    }
    function test_home_controller_launchers() {
        fake.input("press");
        compare(actions[actions.length - 1].type, "open");
        fake.input("right");
        fake.input("press");
        compare(actions[actions.length - 1].name, "steam");
        fake.input("right");
        fake.input("press");
        compare(actions[actions.length - 1].name, "kodi");
    }
    function test_home_library_entry_and_shelf_navigation() {
        route("home", {
            rows: [
                {
                    title: "Continue watching",
                    items: movies(3)
                }
            ]
        });
        compare(shell.activeFocusItem.objectName, "launcher_open");
        fake.input("press");
        compare(actions[actions.length - 1].type, "browse");
        compare(actions[actions.length - 1].kind, "movie");
        fake.input("down");
        compare(shell.activeFocusItem.media.id, 2);
        fake.input("left");
        compare(shell.activeFocusItem.media.id, 1);
        fake.input("press");
        compare(actions[actions.length - 1].type, "detail");
        compare(actions[actions.length - 1].id, 1);
        fake.input("up");
        compare(shell.activeFocusItem.objectName, "launcher_open");
    }
    function test_profile_controller_keyboard() {
        route("login", {
            authenticated: false,
            profiles: [
                {
                    id: 1,
                    name: "Viewer",
                    username: "viewer",
                    passwordless: false
                }
            ]
        });
        fake.input("press");
        compare(findChild(shell, "username").text, "viewer");
        compare(shell.activeFocusItem.objectName, "password");
        fake.input("press");
        wait(40);
        verify(findChild(shell, "keyboard").visible);
        fake.input("press");
        compare(findChild(shell, "password").text, "q");
        fake.input("right");
        fake.input("press");
        compare(findChild(shell, "password").text, "qw");
        fake.input("back");
        verify(!findChild(shell, "keyboard").visible);
        compare(shell.activeFocusItem.objectName, "password");
    }
    function test_artwork_update_preserves_focus() {
        let items = [
            {
                id: 1,
                kind: "movie",
                title: "First",
                art: "a"
            },
            {
                id: 2,
                kind: "movie",
                title: "Second",
                art: "b"
            }
        ];
        route("browse", {
            rows: [
                {
                    title: "Movies",
                    items: items
                }
            ],
            browseKind: "movie",
            browseList: "all"
        });
        fake.input("right");
        compare(shell.activeFocusItem.media.id, 2);
        let state = JSON.parse(JSON.stringify(fake.state));
        state.library.assets = {
            a: ""
        };
        fake.state = state;
        wait(40);
        compare(shell.activeFocusItem.media.id, 2);
        fake.input("press");
        compare(actions[actions.length - 1].id, 2);
    }
    function movies(count) {
        let items = [];
        for (let i = 1; i <= count; i++)
            items.push({
                id: i,
                kind: "movie",
                title: "Movie " + i,
                art: "poster/" + i
            });
        return items;
    }
    function test_grid_rows_append_and_virtualization() {
        route("browse", {
            rows: [
                {
                    title: "Movies",
                    items: movies(30)
                }
            ],
            browseKind: "movie",
            browseList: "all",
            cursor: "next"
        });
        fake.input("down");
        wait(30);
        compare(shell.activeFocusItem.media.id, 8);
        fake.input("right");
        wait(30);
        compare(shell.activeFocusItem.media.id, 9);
        let grid = findChild(shell, "catalogue");
        let scroll = grid.contentY;
        let state = JSON.parse(JSON.stringify(fake.state));
        state.library.rows[0].items = movies(120);
        state.library.cursor = null;
        fake.state = state;
        wait(80);
        compare(shell.activeFocusItem.media.id, 9);
        compare(grid.contentY, scroll);
        compare(grid.count, 120);
        verify(findChild(shell, "catalogueGrid").contentItem.children.length < 50, "The grid must only instantiate nearby cards");
        fake.input("up");
        wait(30);
        compare(shell.activeFocusItem.media.id, 2);
    }
    function test_grid_restores_position_and_requests_more_once() {
        route("browse", {
            rows: [
                {
                    title: "Movies",
                    items: movies(60)
                }
            ],
            browseKind: "movie",
            browseList: "all",
            browseIndex: 25,
            browseScroll: 1002,
            cursor: "next"
        });
        wait(80);
        compare(shell.activeFocusItem.media.id, 26);
        compare(findChild(shell, "catalogue").contentY, 1002);
        let grid = findChild(shell, "catalogue");
        grid.focusAt(59);
        wait(180);
        compare(actions.filter(a => a.type === "more").length, 1);
        fake.input("down");
        wait(160);
        compare(actions.filter(a => a.type === "more").length, 1);
    }
    function test_fast_page_completion_does_not_stall_loading() {
        route("browse", {
            rows: [
                {
                    title: "Movies",
                    items: movies(30)
                }
            ],
            browseKind: "movie",
            browseList: "all",
            cursor: "page2"
        });
        let grid = findChild(shell, "catalogue");
        grid.focusAt(29);
        wait(180);
        compare(actions.filter(a => a.type === "more").length, 1);
        // A fast response can arrive before the UI sees moreLoading=true.
        let state = JSON.parse(JSON.stringify(fake.state));
        state.library.rows[0].items = movies(60);
        state.library.cursor = "page3";
        fake.state = state;
        wait(40);
        grid.focusAt(59);
        wait(180);
        compare(actions.filter(a => a.type === "more").length, 2);
    }
    function test_exit_is_direct_and_home_back_exits() {
        fake.input("back");
        compare(actions[actions.length - 1].type, "exit");
        route("search");
        findChild(shell, "exitTv").clicked();
        compare(actions[actions.length - 1].type, "exit");
        route("home");
        let state = JSON.parse(JSON.stringify(fake.state));
        state.playing = true;
        fake.state = state;
        wait(20);
        fake.input("back");
        compare(actions[actions.length - 1].type, "back");
    }
    function test_series_episodes_have_multiple_rows() {
        let episodes = movies(12);
        for (let item of episodes)
            item.kind = "episode";
        route("detail", {
            detail: {
                id: 8,
                kind: "series",
                title: "Series",
                files: []
            },
            rows: [
                {
                    title: "Season 1",
                    items: episodes
                }
            ]
        });
        let cards = findChild(shell, "episodeGrid").children.filter(c => c.media);
        compare(cards.length, 12);
        verify(cards[4].y > cards[0].y);
        compare(cards[4].x, cards[0].x);
        cards[0].forceActiveFocus();
        fake.input("down");
        wait(40);
        compare(shell.activeFocusItem.media.id, 5);
    }
    function test_native_player_controller_and_track_menus() {
        let state = JSON.parse(JSON.stringify(fake.state));
        // A library refresh must not steal focus from the video controls.
        state.library.serial = serial++;
        state.playing = true;
        state.playerView = true;
        state.playerTitle = "A title";
        state.playback = {
            position: 42,
            duration: 300,
            paused: false,
            buffering: false,
            quality: "original",
            selectedAudio: 4,
            selectedSubtitle: null,
            audioTracks: [
                {
                    index: 4,
                    label: "English"
                },
                {
                    index: 7,
                    label: "German"
                }
            ],
            subtitleTracks: [
                {
                    index: 9,
                    label: "English"
                }
            ]
        };
        fake.state = state;
        wait(140);
        let player = findChild(shell, "playbackView");
        verify(player.visible);
        verify(player.activeFocus);
        keyClick(Qt.Key_Space);
        compare(actions[actions.length - 1].command, "pause");
        fake.input("right");
        compare(actions[actions.length - 1].command, "seek");
        compare(actions[actions.length - 1].value, 10);
        fake.input("press");
        compare(actions[actions.length - 1].command, "pause");
        fake.input("down");
        fake.input("right");
        fake.input("press");
        compare(player.menu, "Audio");
        fake.input("down");
        fake.input("press");
        compare(actions[actions.length - 1].command, "audioTrack");
        compare(actions[actions.length - 1].value, 7);
        verify(player.handleKey(Qt.Key_X));
        fake.input("down");
        fake.input("press");
        compare(actions[actions.length - 1].value, 9);
        compare(actions[actions.length - 1].command, "subtitleTrack");
        player.openMenu("Subtitles");
        fake.input("press");
        compare(actions[actions.length - 1].value, -1);
        player.openMenu("Quality");
        fake.input("down");
        fake.input("down");
        fake.input("press");
        compare(actions[actions.length - 1].command, "setQuality");
        compare(actions[actions.length - 1].value, 1080);
        fake.input("back");
        compare(actions[actions.length - 1].command, "stop");
    }
    function test_source_selection_and_power_hold() {
        route("detail", {
            detail: {
                id: 1,
                kind: "movie",
                title: "Title",
                files: [
                    {
                        label: "Original"
                    },
                    {
                        label: "Alternative"
                    }
                ]
            }
        });
        let source = findChild(shell, "sourcePicker");
        source.forceActiveFocus();
        fake.input("press");
        compare(source.currentIndex, 1);
        route("account");
        // A spatial navigation press/release must reach the hold-only controls without firing power.
        let candidates = [];
        shell.focusables(shell.contentItem, candidates);
        let power = candidates.filter(x => x.hold)[0];
        verify(!!power);
        power.forceActiveFocus();
        fake.input("press");
        fake.input("release");
        verify(actions.some(a => a.type === "powerPress"));
        compare(actions[actions.length - 1].type, "powerRelease");
    }
}
