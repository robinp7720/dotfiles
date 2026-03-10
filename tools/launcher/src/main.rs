mod model;
mod sources;

use crate::model::{Action, ResultItem, SearchMode};
use crate::sources::Sources;
use anyhow::{Context, Result};
use clap::Parser;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Entry, EventControllerKey, Image, Label,
    ListBox, ListBoxRow, Orientation, ScrolledWindow, SelectionMode,
};
use gtk4_layer_shell::LayerShell;
use std::process::Command;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "dot-launcher")]
#[command(about = "Unified desktop launcher for apps, files, SSH, commands, web, and libqalculate")]
struct Cli {
    #[arg(long, value_enum, default_value_t = SearchMode::All)]
    mode: SearchMode,

    #[arg(long)]
    query: Option<String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let sources = Arc::new(Sources::load());
    let application = Application::builder()
        .application_id("me.robindecker.DotLauncher")
        .build();

    application.connect_activate(move |app| {
        build_ui(app, cli.mode, cli.query.clone(), sources.clone());
    });

    application.run_with_args(&["dot-launcher"]);
    Ok(())
}

fn build_ui(
    app: &Application,
    mode: SearchMode,
    initial_query: Option<String>,
    sources: Arc<Sources>,
) {
    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(820)
        .default_height(520)
        .decorated(false)
        .resizable(false)
        .title("Launcher")
        .build();

    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Overlay);
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    window.set_namespace(Some("dot-launcher"));
    window.set_anchor(gtk4_layer_shell::Edge::Top, true);
    window.set_margin(gtk4_layer_shell::Edge::Top, 140);

    apply_css();

    let outer = GtkBox::new(Orientation::Vertical, 14);
    outer.add_css_class("launcher-shell");
    outer.set_halign(Align::Center);
    outer.set_size_request(820, -1);
    outer.set_margin_top(22);
    outer.set_margin_bottom(22);
    outer.set_margin_start(22);
    outer.set_margin_end(22);

    let entry = Entry::builder()
        .placeholder_text(placeholder_for_mode(mode))
        .build();
    entry.add_css_class("launcher-entry");
    if let Some(query) = initial_query.as_deref() {
        entry.set_text(query);
    }

    let hint = Label::new(Some(
        "Type to search apps, files, SSH hosts, commands, web, and calculations",
    ));
    hint.add_css_class("launcher-hint");
    hint.set_halign(Align::Start);

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.add_css_class("launcher-results");

    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .min_content_height(360)
        .child(&list)
        .build();
    scroller.add_css_class("launcher-scroller");

    outer.append(&entry);
    outer.append(&hint);
    outer.append(&scroller);
    window.set_child(Some(&outer));

    {
        let sources = sources.clone();
        let list = list.clone();
        entry.connect_changed(move |entry| {
            let query = entry.text().to_string();
            let results = sources.search(&query, mode);
            rebuild_results(&list, &results);
        });
    }

    {
        let sources = sources.clone();
        let entry = entry.clone();
        let window = window.clone();
        list.connect_row_activated(move |_, row| {
            let query = entry.text().to_string();
            let results = sources.search(&query, mode);
            activate_row(row, &results, &window);
        });
    }

    {
        let sources = sources.clone();
        let entry = entry.clone();
        let window = window.clone();
        let activate_entry = entry.clone();
        entry.connect_activate(move |_| {
            let query = activate_entry.text().to_string();
            let results = sources.search(&query, mode);
            if let Some(item) = results.first().cloned() {
                let _ = execute_action(&window, item.action);
            }
        });
    }

    {
        let entry = entry.clone();
        let list = list.clone();
        let sources = sources.clone();
        let window = window.clone();
        let keys = EventControllerKey::new();
        let key_window = window.clone();
        keys.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                key_window.close();
                glib::Propagation::Stop
            }
            gdk::Key::Down => {
                let results = sources.search(&entry.text(), mode);
                move_selection(&list, 1, results.len() as i32);
                entry.grab_focus();
                glib::Propagation::Stop
            }
            gdk::Key::Up => {
                let results = sources.search(&entry.text(), mode);
                move_selection(&list, -1, results.len() as i32);
                entry.grab_focus();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        window.add_controller(keys);
    }

    let initial_results = sources.search(initial_query.as_deref().unwrap_or_default(), mode);
    rebuild_results(&list, &initial_results);

    window.present();
    entry.grab_focus();
}

fn rebuild_results(list: &ListBox, results: &[ResultItem]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for item in results {
        let row = build_row(item);
        list.append(&row);
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
}

fn build_row(item: &ResultItem) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("launcher-row");

    let layout = GtkBox::new(Orientation::Horizontal, 14);
    layout.set_margin_top(10);
    layout.set_margin_bottom(10);
    layout.set_margin_start(12);
    layout.set_margin_end(12);

    let icon = Image::from_icon_name(&item.icon_name);
    icon.set_pixel_size(28);

    let text_box = GtkBox::new(Orientation::Vertical, 4);

    let title = Label::new(Some(&item.title));
    title.add_css_class("launcher-title");
    title.set_halign(Align::Start);
    title.set_xalign(0.0);

    let subtitle = Label::new(Some(&format!("{}  •  {}", item.source, item.subtitle)));
    subtitle.add_css_class("launcher-subtitle");
    subtitle.set_halign(Align::Start);
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);

    text_box.append(&title);
    text_box.append(&subtitle);

    layout.append(&icon);
    layout.append(&text_box);
    row.set_child(Some(&layout));
    row
}

fn move_selection(list: &ListBox, delta: i32, result_count: i32) {
    if result_count <= 0 {
        return;
    }

    let current = list.selected_row().map(|row| row.index()).unwrap_or(0);
    let next = (current + delta).clamp(0, result_count - 1);
    if let Some(row) = list.row_at_index(next) {
        list.select_row(Some(&row));
    }
}

fn activate_row(row: &ListBoxRow, results: &[ResultItem], window: &ApplicationWindow) {
    let index = row.index() as usize;
    if let Some(item) = results.get(index).cloned() {
        let _ = execute_action(window, item.action);
    }
}

fn execute_action(window: &ApplicationWindow, action: Action) -> Result<()> {
    match action {
        Action::LaunchApp { desktop_id } => launch_desktop_app(&desktop_id)?,
        Action::OpenFile { path } => {
            let file = gio::File::for_path(path);
            gio::AppInfo::launch_default_for_uri(&file.uri(), gio::AppLaunchContext::NONE)
                .context("failed to open file")?;
        }
        Action::Ssh { host } => launch_ssh(&host)?,
        Action::RunCommand { command } => {
            Command::new("sh")
                .args(["-lc", &command])
                .spawn()
                .context("failed to spawn command")?;
        }
        Action::WebSearch { query } => {
            let base = std::env::var("DOT_LAUNCHER_SEARCH_URL")
                .unwrap_or_else(|_| "https://duckduckgo.com/?q=".to_string());
            let url = format!("{base}{}", urlencoding::encode(&query));
            gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE)
                .context("failed to open search URL")?;
        }
        Action::CopyText { text } => {
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text(&text);
            }
        }
        Action::None => {}
    }

    window.close();
    Ok(())
}

fn launch_desktop_app(desktop_id: &str) -> Result<()> {
    if let Some(app) = gio::DesktopAppInfo::new(desktop_id) {
        app.launch(&[], gio::AppLaunchContext::NONE)
            .context("failed to launch desktop app")?;
        return Ok(());
    }

    let app = gio::AppInfo::all()
        .into_iter()
        .find(|app| app.id().as_deref() == Some(desktop_id))
        .context("desktop application no longer exists")?;
    app.launch(&[], gio::AppLaunchContext::NONE)
        .context("failed to launch desktop app")?;
    Ok(())
}

fn launch_ssh(host: &str) -> Result<()> {
    let terminal = dirs::home_dir()
        .map(|path| path.join(".dotfiles/scripts/launch_kitty.sh"))
        .context("home directory not available")?;

    Command::new(terminal)
        .args(["-e", "ssh", host])
        .spawn()
        .context("failed to launch ssh session")?;
    Ok(())
}

fn placeholder_for_mode(mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::All => "Search apps, files, SSH, commands, web, and calculations",
        SearchMode::Apps => "Launch an application",
        SearchMode::Files => "Search files with tracker3",
        SearchMode::Ssh => "Search SSH hosts",
        SearchMode::Commands => "Run a command",
        SearchMode::Web => "Search the web",
        SearchMode::Calc => "Evaluate a libqalculate expression",
    }
}

fn apply_css() {
    let css = r#"
      window {
        background: transparent;
      }

      .launcher-shell {
        background: rgba(18, 20, 28, 0.90);
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 12px;
        box-shadow: 0 20px 48px rgba(0, 0, 0, 0.34);
      }

      .launcher-entry {
        min-height: 58px;
        font-size: 1.28rem;
        padding: 0.35rem 0.8rem;
        border-radius: 10px;
      }

      .launcher-hint {
        color: rgba(255, 255, 255, 0.68);
        font-size: 0.92rem;
        margin-left: 0.25rem;
      }

      .launcher-results {
        background: transparent;
      }

      .launcher-row {
        margin-bottom: 6px;
        border-radius: 10px;
      }

      .launcher-row:selected {
        background: rgba(255, 255, 255, 0.10);
      }

      .launcher-title {
        font-size: 1.05rem;
        font-weight: 700;
      }

      .launcher-subtitle {
        color: rgba(255, 255, 255, 0.70);
        font-size: 0.92rem;
      }
    "#;

    let provider = gtk4::CssProvider::new();
    provider.load_from_string(css);
    gtk4::style_context_add_provider_for_display(
        &gdk::Display::default().expect("display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
