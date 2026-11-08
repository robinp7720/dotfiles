import QtQuick
import QtTest
import "../../qml" as TV

// Offline fixtures: no credentials, artwork downloads, or desktop/session changes.
TestCase {
    id: test
    name: "TVScreens"
    when: windowShown
    property int serial: 1
    property var assets: ({})
    property var movies: []
    property var episodes: []
    property var series: []
    QtObject {
        id: fake
        property var state: ({
                library: {
                    page: "home",
                    serial: 0,
                    rows: [],
                    assets: {},
                    profiles: []
                },
                reducedMotion: true
            })
        signal input(string key)
        function send(action) {
        }
    }
    TV.Main {
        id: shell
        bridge: fake
        visible: true
    }
    function art(title, index, wide) {
        let palette = [["#668594", "#213544"], ["#b98361", "#422e36"], ["#83977a", "#233c3a"], ["#8b859f", "#343344"]][index % 4];
        let w = wide ? 1600 : 440, h = wide ? 900 : 660;
        let svg = '<svg xmlns="http://www.w3.org/2000/svg" width="' + w + '" height="' + h + '" viewBox="0 0 1600 900">' + '<defs><linearGradient id="sky" x2="0" y2="1"><stop stop-color="' + palette[0] + '"/><stop offset="1" stop-color="' + palette[1] + '"/></linearGradient></defs>' + '<rect width="1600" height="900" fill="url(#sky)"/><circle cx="1120" cy="230" r="120" fill="#e2cda1" opacity=".55"/>' + '<path d="M0 650 350 220 720 600 1080 330 1600 720V900H0Z" fill="' + palette[1] + '"/><path d="M0 780 420 560 900 810 1320 580 1600 750V900H0Z" fill="#101c23" opacity=".8"/>' + (wide ? '' : '<text x="800" y="730" text-anchor="middle" font-family="sans-serif" font-size="100" fill="white">' + title + '</text>') + '</svg>';
        return "data:image/svg+xml," + encodeURIComponent(svg);
    }
    function initTestCase() {
        let names = ["Northbound", "The last light", "Quiet waters", "After the rain", "Far from home", "The crossing", "Nightfall", "A new season"];
        let pictures = {}, films = [], shows = [], eps = [];
        for (let i = 0; i < 30; i++) {
            let title = names[i % names.length];
            pictures["poster/" + i] = art(title, i, false);
            pictures["backdrop/" + i] = art(title, i, true);
            films.push({
                id: i + 1,
                kind: "movie",
                title: title,
                subtitle: "2025 · 1h 48m",
                overview: "Two travellers return to the coast where they grew up, following a trail of letters that changes what they remember about home.",
                art: "poster/" + i,
                backdrop: "backdrop/" + i,
                progress: i === 0 ? 0.42 : 0
            });
            if (i < 12)
                eps.push({
                    id: 100 + i,
                    kind: "episode",
                    title: ["The long way home", "A place to begin", "The other shore", "Before the dawn"][i % 4],
                    subtitle: "Northbound · S1 · E" + (i + 1),
                    art: "backdrop/" + i,
                    backdrop: "backdrop/" + i,
                    overview: films[i].overview,
                    progress: i === 0 ? 0.32 : 0
                });
            if (i < 6)
                shows.push({
                    id: 50 + i,
                    kind: "series",
                    title: title,
                    subtitle: "Drama · 2025",
                    art: "poster/" + i,
                    backdrop: "backdrop/" + i
                });
        }
        assets = pictures;
        movies = films;
        series = shows;
        episodes = eps;
    }
    function route(page, extra, motion = false) {
        let lib = {
            page: page,
            serial: serial++,
            rows: [],
            assets: assets,
            profiles: [],
            authenticated: true,
            account: "Robin",
            server: "https://media.example.test",
            loading: false
        };
        if (extra)
            for (let key in extra)
                lib[key] = extra[key];
        fake.state = {
            library: lib,
            reducedMotion: !motion,
            controller: "Controller connected"
        };
        shell.requestActivate();
        wait(400);
    }
    function capture(name, delay = 100) {
        wait(delay);
        let image = grabImage(shell.contentItem);
        compare(image.width, shell.width);
        image.save("/tmp/tv-mode-shell-qa/" + name + ".png");
    }
    function test_motion_frames() {
        route("home", {
            rows: [
                {
                    title: "Continue watching",
                    items: episodes.slice(0, 4)
                }
            ]
        }, true);
        let targets = [];
        shell.focusables(shell.contentItem, targets);
        targets.find(item => item.objectName === "nav_home").forceActiveFocus();
        capture("navigation-entering", 60);
        capture("navigation-settled", 240);
        route("search", {}, true);
        shell.openKeyboard(findChild(shell, "searchField"));
        capture("keyboard-entering", 60);
        capture("keyboard-settled", 240);
        findChild(shell, "keyboard").finish();
        capture("keyboard-exiting", 40);
    }
    function test_screens() {
        route("home", {
            rows: [
                {
                    title: "Continue watching",
                    items: episodes.slice(0, 4)
                },
                {
                    title: "Movies",
                    items: movies.slice(0, 8)
                },
                {
                    title: "Series",
                    items: series
                }
            ]
        });
        capture("home");
        let targets = [];
        shell.focusables(shell.contentItem, targets);
        targets.find(item => item.objectName === "nav_home").forceActiveFocus();
        capture("navigation");
        route("browse", {
            title: "Movies",
            rows: [
                {
                    title: "Movies",
                    items: movies
                }
            ],
            browseKind: "movie",
            browseList: "all"
        });
        capture("browse");
        route("browse", {
            title: "Watchlist",
            rows: [],
            browseKind: "movie",
            browseList: "watchlist"
        });
        capture("empty-watchlist");
        route("detail", {
            detail: Object.assign({}, movies[0], {
                resumable: true,
                resume: "42:16",
                files: [
                    {
                        label: "Original · 4K · English"
                    },
                    {
                        label: "1080p · English"
                    }
                ]
            })
        });
        capture("movie");
        route("detail", {
            detail: Object.assign({}, series[0], {
                overview: movies[0].overview,
                files: []
            }),
            rows: [
                {
                    title: "Season 1",
                    items: episodes
                },
                {
                    title: "Season 2",
                    items: episodes.slice(0, 4)
                }
            ]
        });
        capture("series");
        let card = findChild(shell, "episodeGrid").children.filter(c => c.media)[4];
        card.forceActiveFocus();
        shell.ensureVisible(card);
        wait(200);
        capture("episodes");
        route("search");
        capture("search");
        findChild(shell, "searchField").text = "North";
        fake.input("press");
        wait(100);
        capture("keyboard");
        route("login", {
            authenticated: false,
            profiles: [
                {
                    id: 1,
                    name: "Robin",
                    username: "robin",
                    passwordless: false
                },
                {
                    id: 2,
                    name: "Alex",
                    username: "alex",
                    passwordless: false
                },
                {
                    id: 3,
                    name: "Guest",
                    username: "guest",
                    passwordless: true
                }
            ]
        });
        capture("profiles");
        route("login", {
            authenticated: false
        });
        capture("signin");
        route("account");
        capture("settings");
        shell.width = 960;
        shell.height = 540;
        wait(100);
        capture("settings-960");
        shell.width = 1600;
        shell.height = 900;
        route("home", {
            authenticated: false
        });
        capture("guest-home");
    }
}
