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

#include <QQuickFramebufferObject>
#include <QOpenGLContext>
#include <QSurfaceFormat>
#include <QOpenGLFramebufferObject>
#include <QOpenGLFunctions>
#include <QOpenGLExtraFunctions>
#include <QPointer>
#include <mpv/client.h>
#include <mpv/render_gl.h>
#include <memory>
#include <stdexcept>
#include <clocale>

// The GUI owns the core; each renderer keeps it alive until its GL context is freed.
struct VideoCore {
    mpv_handle *handle = nullptr;
    explicit VideoCore(const char *socket) {
        handle = mpv_create();
        if (!handle) throw std::runtime_error("Could not create the video engine");
        for (auto option : {std::pair{"vo", "libmpv"}, {"terminal", "no"},
                {"config", "no"}, {"idle", "yes"}, {"keep-open", "no"},
                {"osc", "no"}, {"osd-level", "0"}, {"input-default-bindings", "no"},
                {"load-scripts", "no"}, {"ytdl", "no"}, {"sub-auto", "no"},
                {"audio-file-auto", "no"}, {"msg-level", "all=no"}}) {
            if (mpv_set_option_string(handle, option.first, option.second) < 0) {
                mpv_terminate_destroy(handle);
                handle = nullptr;
                throw std::runtime_error("Could not configure the video engine");
            }
        }
        mpv_set_option_string(handle, "hwdec", qEnvironmentVariableIntValue("TV_MODE_SOFTWARE_VIDEO") == 1 ? "no" : "auto-safe");
        mpv_set_option_string(handle, "input-ipc-server", socket);
        if (mpv_initialize(handle) < 0) {
            mpv_terminate_destroy(handle);
            handle = nullptr;
            throw std::runtime_error("Could not initialize the video engine");
        }
    }
    ~VideoCore() { if (handle) mpv_terminate_destroy(handle); }
};
static std::weak_ptr<VideoCore> videoCore;

class VideoSurface : public QQuickFramebufferObject {
    Q_OBJECT
public:
    std::shared_ptr<VideoCore> core = videoCore.lock();
    Renderer *createRenderer() const override;
signals:
    void initialized(bool success);
};
class VideoRenderer : public QQuickFramebufferObject::Renderer {
    std::shared_ptr<VideoCore> core;
    mpv_render_context *renderContext = nullptr;
    std::unique_ptr<QOpenGLContext> videoContext;
    GLuint videoFramebuffer = 0;
    bool initialized = false;
    QPointer<VideoSurface> surface;
    static void *resolve(void *, const char *name) {
        return reinterpret_cast<void *>(QOpenGLContext::currentContext()->getProcAddress(name));
    }
    static void wake(void *opaque) {
        auto renderer = static_cast<VideoRenderer *>(opaque);
        auto target = renderer->surface;
        // Call the framebuffer item's update(), not QQuickItem's inherited meta-method.
        if (target)
            QMetaObject::invokeMethod(target, [target] { if (target) target->update(); }, Qt::QueuedConnection);
    }
    void initialize(QOpenGLContext *qtContext, QSurface *targetSurface) {
        initialized = true;
        // Share textures, but isolate decoder GL state from Qt's renderer and caches.
        videoContext = std::make_unique<QOpenGLContext>();
        videoContext->setFormat(qtContext->format());
        videoContext->setShareContext(qtContext);
        bool current = videoContext->create() && videoContext->makeCurrent(targetSurface);
        mpv_opengl_init_params gl{resolve, nullptr};
        mpv_render_param params[] = {
            {MPV_RENDER_PARAM_API_TYPE, const_cast<char *>(MPV_RENDER_API_TYPE_OPENGL)},
            {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &gl}, {MPV_RENDER_PARAM_INVALID, nullptr}
        };
        bool success = current && core && mpv_render_context_create(&renderContext, core->handle, params) >= 0;
        if (success) {
            videoContext->functions()->glGenFramebuffers(1, &videoFramebuffer);
            mpv_render_context_set_update_callback(renderContext, wake, this);
        }
        auto target = surface;
        if (target)
            QMetaObject::invokeMethod(target, [target, success] { if (target) emit target->initialized(success); }, Qt::QueuedConnection);
    }
public:
    explicit VideoRenderer(VideoSurface *item) : core(item->core), surface(item) {}
    ~VideoRenderer() override {
        auto qtContext = QOpenGLContext::currentContext();
        auto targetSurface = qtContext ? qtContext->surface() : nullptr;
        if (videoContext && targetSurface) videoContext->makeCurrent(targetSurface);
        if (renderContext) {
            mpv_render_context_set_update_callback(renderContext, nullptr, nullptr);
            mpv_render_context_free(renderContext);
        }
        if (videoFramebuffer) videoContext->functions()->glDeleteFramebuffers(1, &videoFramebuffer);
        if (qtContext && targetSurface) qtContext->makeCurrent(targetSurface);
    }
    void render() override {
        auto qtContext = QOpenGLContext::currentContext();
        auto targetSurface = qtContext->surface();
        qtContext->functions()->glFlush();
        if (!initialized) initialize(qtContext, targetSurface);
        auto fbo = framebufferObject();
        if (renderContext && videoContext->makeCurrent(targetSurface)) {
            auto gl = videoContext->extraFunctions();
            // FBOs are context-local; their attached texture is shared with Qt.
            gl->glBindFramebuffer(GL_FRAMEBUFFER, videoFramebuffer);
            gl->glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, fbo->texture(), 0);
            mpv_opengl_fbo target{static_cast<int>(videoFramebuffer), fbo->width(), fbo->height(), GL_RGBA8};
            // Explicit color depth avoids incorrect dithering on offscreen GL targets.
            int flip = 0, depth = 8;
            mpv_render_param params[] = {
                {MPV_RENDER_PARAM_OPENGL_FBO, &target}, {MPV_RENDER_PARAM_FLIP_Y, &flip},
                {MPV_RENDER_PARAM_DEPTH, &depth},
                {MPV_RENDER_PARAM_INVALID, nullptr}
            };
            mpv_render_context_update(renderContext);
            mpv_render_context_render(renderContext, params);
            bool sync = videoContext->format().version() >= qMakePair(3, 2)
                || (videoContext->isOpenGLES() && videoContext->format().majorVersion() >= 3)
                || videoContext->hasExtension("GL_ARB_sync");
            GLsync fence = nullptr;
            if (sync) {
                fence = gl->glFenceSync(GL_SYNC_GPU_COMMANDS_COMPLETE, 0);
                gl->glFlush();
            } else gl->glFinish();
            qtContext->makeCurrent(targetSurface);
            if (fence) {
                auto qtGl = qtContext->extraFunctions();
                qtGl->glWaitSync(fence, 0, GL_TIMEOUT_IGNORED);
                qtGl->glDeleteSync(fence);
            }
        } else {
            qtContext->makeCurrent(targetSurface);
            fbo->bind();
            auto gl = qtContext->functions();
            gl->glClearColor(0, 0, 0, 1);
            gl->glClear(GL_COLOR_BUFFER_BIT);
        }
    }
};
QQuickFramebufferObject::Renderer *VideoSurface::createRenderer() const {
    return new VideoRenderer(const_cast<VideoSurface *>(this));
}

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
extern "C" int tv_quick_run(void *context, Poll poll, Action action, const char *output, bool preview, const char *socket) {
    initResources();
    int argc = 1;
    char name[] = "tv-mode";
    char *argv[] = {name, nullptr};
    qputenv("QT_QUICK_CONTROLS_STYLE", "Basic");
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
    QSurfaceFormat format;
    // Wayland may default to OpenGL ES, where the requested desktop 3.3 core
    // context is invalid (EGL_BAD_MATCH). Match the API to the version/profile.
    format.setRenderableType(QSurfaceFormat::OpenGL);
    format.setVersion(3, 3);
    format.setProfile(QSurfaceFormat::CoreProfile);
    QSurfaceFormat::setDefaultFormat(format);
    QGuiApplication app(argc, argv);
    // Qt adopts the desktop locale; libmpv requires C numeric parsing.
    std::setlocale(LC_NUMERIC, "C");
    std::shared_ptr<VideoCore> core;
    try { core = std::make_shared<VideoCore>(socket); }
    catch (const std::exception &e) { qCritical("%s", e.what()); return 4; }
    videoCore = core;
    qmlRegisterType<VideoSurface>("TvMode.Video", 1, 0, "VideoSurface");
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
    window->setPersistentGraphics(true);
    window->setPersistentSceneGraph(true);
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
        if (stopping) {
            stopping = 0;
            bridge.send({{"type", "shutdown"}});
        }
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
