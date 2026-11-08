#include <gtk/gtk.h>
#include <gtk4-layer-shell.h>
#include <glib-unix.h>
#include <unistd.h>

// A separate process keeps GTK and Qt event loops independent. EOF from the
// owner reveals the shell (or the desktop after rollback), even if it crashes.
typedef struct {
    GMainLoop *loop;
    GPtrArray *windows;
    gint64 started;
    gint64 reveal_started;
    double alpha;
    double reveal_from;
    guint painted;
    gboolean ready;
} Curtain;

static void draw(GtkDrawingArea *area, cairo_t *cr, int width, int height, gpointer data) {
    (void) width;
    (void) height;
    Curtain *curtain = data;
    cairo_set_source_rgba(cr, 0, 0, 0, curtain->alpha);
    cairo_paint(cr);
    if (curtain->alpha == 1 && !g_object_get_data(G_OBJECT(area), "painted")) {
        g_object_set_data(G_OBJECT(area), "painted", GINT_TO_POINTER(1));
        curtain->painted++;
    }
}

static void passthrough(GtkWidget *widget, gpointer data) {
    (void) data;
    cairo_region_t *empty = cairo_region_create();
    gdk_surface_set_input_region(gtk_native_get_surface(GTK_NATIVE(widget)), empty);
    cairo_region_destroy(empty);
}

static void reveal(Curtain *curtain) {
    if (!curtain->reveal_started) {
        curtain->reveal_started = g_get_monotonic_time();
        curtain->reveal_from = curtain->alpha;
    }
}

static gboolean owner_closed(gint fd, GIOCondition condition, gpointer data) {
    (void) fd;
    (void) condition;
    reveal(data);
    return G_SOURCE_REMOVE;
}

static gboolean tick(gpointer data) {
    Curtain *curtain = data;
    gint64 now = g_get_monotonic_time();
    // Never strand a black overlay if the shell fails to present a frame.
    if (now - curtain->started > 15000000)
        reveal(curtain);
    if (!curtain->ready && curtain->painted == curtain->windows->len) {
        curtain->ready = TRUE;
        if (write(STDOUT_FILENO, "R", 1) != 1)
            reveal(curtain);
    }
    if (curtain->reveal_started) {
        double t = MIN(1.0, (now - curtain->reveal_started) / 320000.0);
        curtain->alpha = curtain->reveal_from * (1 - t) * (1 - t) * (1 - t);
        if (t == 1) {
            g_main_loop_quit(curtain->loop);
            return G_SOURCE_REMOVE;
        }
    } else {
        double t = MIN(1.0, (now - curtain->started) / 240000.0);
        curtain->alpha = t * t * (3 - 2 * t);
    }
    for (guint i = 0; i < curtain->windows->len; i++)
        gtk_widget_queue_draw(gtk_window_get_child(g_ptr_array_index(curtain->windows, i)));
    return G_SOURCE_CONTINUE;
}

int tv_transition_run(void) {
    if (!gtk_init_check() || !gtk_layer_is_supported())
        return 1;
    gboolean animations = TRUE;
    g_object_get(gtk_settings_get_default(), "gtk-enable-animations", &animations, NULL);
    if (!animations)
        return 0;
    Curtain curtain = {0};
    curtain.loop = g_main_loop_new(NULL, FALSE);
    curtain.windows = g_ptr_array_new();
    GtkCssProvider *css = gtk_css_provider_new();
    gtk_css_provider_load_from_string(css, "window { background: transparent; box-shadow: none; }");
    gtk_style_context_add_provider_for_display(gdk_display_get_default(), GTK_STYLE_PROVIDER(css),
                                               GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
    GListModel *monitors = gdk_display_get_monitors(gdk_display_get_default());
    for (guint i = 0; i < g_list_model_get_n_items(monitors); i++) {
        GdkMonitor *monitor = g_list_model_get_item(monitors, i);
        GtkWindow *window = GTK_WINDOW(gtk_window_new());
        gtk_layer_init_for_window(window);
        gtk_layer_set_namespace(window, "tv-mode-transition");
        gtk_layer_set_layer(window, GTK_LAYER_SHELL_LAYER_OVERLAY);
        gtk_layer_set_monitor(window, monitor);
        gtk_layer_set_exclusive_zone(window, -1);
        gtk_layer_set_keyboard_mode(window, GTK_LAYER_SHELL_KEYBOARD_MODE_NONE);
        for (int edge = 0; edge < GTK_LAYER_SHELL_EDGE_ENTRY_NUMBER; edge++)
            gtk_layer_set_anchor(window, edge, TRUE);
        GtkWidget *area = gtk_drawing_area_new();
        gtk_drawing_area_set_draw_func(GTK_DRAWING_AREA(area), draw, &curtain, NULL);
        gtk_window_set_child(window, area);
        g_signal_connect(window, "map", G_CALLBACK(passthrough), NULL);
        gtk_window_present(window);
        g_ptr_array_add(curtain.windows, window);
        g_object_unref(monitor);
    }
    if (!curtain.windows->len)
        return 1;
    curtain.started = g_get_monotonic_time();
    g_unix_fd_add(STDIN_FILENO, G_IO_IN | G_IO_HUP | G_IO_ERR, owner_closed, &curtain);
    g_timeout_add(16, tick, &curtain);
    g_main_loop_run(curtain.loop);
    for (guint i = 0; i < curtain.windows->len; i++)
        gtk_window_destroy(g_ptr_array_index(curtain.windows, i));
    g_ptr_array_free(curtain.windows, TRUE);
    g_main_loop_unref(curtain.loop);
    g_object_unref(css);
    return 0;
}
