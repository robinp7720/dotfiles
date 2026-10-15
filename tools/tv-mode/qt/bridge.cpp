#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QScreen>
#include <QTimer>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <csignal>
#include <cstdio>
#include <QQmlError>

using Poll = const char *(*)(void *);
using Action = void (*)(void *, const char *);
static volatile std::sig_atomic_t stopping = 0;
static void stopSignal(int) { stopping = 1; }
class Bridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap state READ state NOTIFY stateChanged)
public:
    void *context;
    Action action;
    QVariantMap data;
    QVariantMap state() const { return data; }
    Q_INVOKABLE void send(const QVariantMap &value) {
        auto bytes = QJsonDocument::fromVariant(value).toJson(QJsonDocument::Compact);
        action(context, bytes.constData());
    }
signals:
    void stateChanged();
    void input(QString key);
};
static void initResources() { Q_INIT_RESOURCE(resources); }
extern "C" int tv_quick_run(void *context, Poll poll, Action action, const char *output, bool preview) {
    initResources();
    int argc = 1;
    char name[] = "tv-mode";
    char *argv[] = {name, nullptr};
    qputenv("QT_QUICK_CONTROLS_STYLE", "Basic");
    QGuiApplication app(argc, argv);
    app.setApplicationName("tv-mode");
    app.setDesktopFileName(preview ? "me.robindecker.TvMode.Preview" : "me.robindecker.TvMode");
    auto oldTerm = std::signal(SIGTERM, stopSignal);
    auto oldInt = std::signal(SIGINT, stopSignal);
    stopping = 0;
    Bridge bridge;
    bridge.context = context;
    bridge.action = action;
    QQmlApplicationEngine engine;
    auto initial = QJsonDocument::fromJson(poll(context)).object();
    bridge.data = initial["state"].toObject().toVariantMap();
    engine.rootContext()->setContextProperty("backend", &bridge);
    QObject::connect(&engine, &QQmlApplicationEngine::warnings, [](const QList<QQmlError> &errors) {
        for (const auto &error : errors) fprintf(stderr, "%s\n", qPrintable(error.toString()));
    });
    engine.load(QUrl("qrc:/Main.qml"));
    if (engine.rootObjects().isEmpty()) return 1;
    auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
    if (!window) return 2;
    if (!preview) {
        QScreen *target = nullptr;
        for (auto screen : app.screens()) if (screen->name() == output) target = screen;
        if (!target) { qCritical("TV output not found: %s", output); return 3; }
        window->setScreen(target);
        window->setPosition(target->geometry().topLeft());
        window->showFullScreen();
    } else window->show();
    QTimer timer;
    QObject::connect(&timer, &QTimer::timeout, &app, [&] {
        if (stopping) { app.exit(0); return; }
        const char *bytes = poll(context);
        if (!bytes) return;
        auto update = QJsonDocument::fromJson(bytes).object();
        if (update.contains("state")) {
            bridge.data = update["state"].toObject().toVariantMap();
            emit bridge.stateChanged();
        }
        for (auto key : update["inputs"].toArray()) emit bridge.input(key.toString());
        auto visibility = update["window"].toString();
        if (visibility == "hide") window->hide();
        if (visibility == "show") {
            if (preview) window->show(); else window->showFullScreen();
            window->raise();
            window->requestActivate();
        }
        if (update["quit"].toBool()) app.exit(0);
    });
    timer.start(16);
    int result = app.exec();
    std::signal(SIGTERM, oldTerm);
    std::signal(SIGINT, oldInt);
    return result;
}
#include "bridge.moc"
