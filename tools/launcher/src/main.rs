mod model;
mod sources;

use crate::model::{Action, QueryInput, ResultItem, SearchMode};
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

    let hint = Label::new(Some("Prefixes: > commands, / files, @ ssh, ? web, = calc"));
    hint.add_css_class("launcher-hint");
    hint.set_halign(Align::Start);
    hint.set_hexpand(true);
    hint.set_wrap(true);

    let mode_badge = Label::new(Some(mode.label()));
    mode_badge.add_css_class("launcher-mode-badge");
    mode_badge.set_halign(Align::End);

    let meta = GtkBox::new(Orientation::Horizontal, 12);
    meta.set_halign(Align::Fill);
    meta.append(&hint);
    meta.append(&mode_badge);

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
    outer.append(&meta);
    outer.append(&scroller);
    window.set_child(Some(&outer));

    {
        let sources = sources.clone();
        let list = list.clone();
        let hint = hint.clone();
        let mode_badge = mode_badge.clone();
        entry.connect_changed(move |entry| {
            let query = entry.text().to_string();
            update_search_meta(&hint, &mode_badge, &query, mode);
            let results = sources.search(&query, mode);
            rebuild_results(&list, &results);
        });
    }

    {
        let sources = sources.clone();
        let entry = entry.clone();
        let hint = hint.clone();
        let window = window.clone();
        list.connect_row_activated(move |_, row| {
            let query = entry.text().to_string();
            let results = sources.search(&query, mode);
            activate_row(row, &results, &window, &hint);
        });
    }

    {
        let sources = sources.clone();
        let entry = entry.clone();
        let hint = hint.clone();
        let list = list.clone();
        let window = window.clone();
        let activate_entry = entry.clone();
        entry.connect_activate(move |_| {
            let query = activate_entry.text().to_string();
            let results = sources.search(&query, mode);
            let selected = list
                .selected_row()
                .and_then(|row| results.get(row.index() as usize).cloned())
                .or_else(|| results.first().cloned());

            if let Some(item) = selected {
                if let Err(error) = execute_action(&window, item.action) {
                    set_hint_state(
                        &hint,
                        &format!("Action failed: {}", error.root_cause()),
                        true,
                    );
                }
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
    update_search_meta(
        &hint,
        &mode_badge,
        initial_query.as_deref().unwrap_or_default(),
        mode,
    );
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
    if matches!(&item.action, Action::None) {
        row.add_css_class("launcher-row-status");
    }

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
    title.set_wrap(true);

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

fn activate_row(
    row: &ListBoxRow,
    results: &[ResultItem],
    window: &ApplicationWindow,
    hint: &Label,
) {
    let index = row.index() as usize;
    if let Some(item) = results.get(index).cloned() {
        if let Err(error) = execute_action(window, item.action) {
            set_hint_state(
                hint,
                &format!("Action failed: {}", error.root_cause()),
                true,
            );
        }
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
        Action::None => return Ok(()),
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
    let terminal = std::env::var("DOT_LAUNCHER_TERMINAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_ssh_terminal(dirs::home_dir().as_deref()));

    Command::new(&terminal)
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

fn update_search_meta(hint: &Label, mode_badge: &Label, raw_query: &str, cli_mode: SearchMode) {
    let query = QueryInput::parse(raw_query, cli_mode);
    mode_badge.set_text(query.mode.label());

    let hint_text = match query.mode {
        SearchMode::All => "Prefixes: > commands, / files, @ ssh, ? web, = calc",
        SearchMode::Apps => "Search installed desktop applications",
        SearchMode::Files => "Search tracker3 indexed files",
        SearchMode::Ssh => "Search aliases from ~/.ssh/config and known_hosts",
        SearchMode::Commands => "Press Enter to run the command or choose a suggestion",
        SearchMode::Web => "Press Enter to open the browser with your query",
        SearchMode::Calc => "Press Enter to copy the calculated result",
    };
    set_hint_state(hint, hint_text, false);
}

fn set_hint_state(hint: &Label, message: &str, is_error: bool) {
    hint.set_text(message);
    if is_error {
        hint.add_css_class("launcher-hint-error");
    } else {
        hint.remove_css_class("launcher-hint-error");
    }
}

fn default_ssh_terminal(home: Option<&std::path::Path>) -> String {
    if let Some(home) = home {
        let launcher = home.join(".dotfiles/scripts/launch_kitty.sh");
        if is_executable(&launcher) {
            return launcher.to_string_lossy().to_string();
        }
    }

    "kitty".to_string()
}

fn is_executable(path: &std::path::Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };

    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
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

      .launcher-hint-error {
        color: rgba(255, 187, 187, 0.96);
      }

      .launcher-mode-badge {
        background: rgba(148, 197, 255, 0.14);
        border: 1px solid rgba(148, 197, 255, 0.26);
        border-radius: 999px;
        color: rgba(214, 231, 255, 0.96);
        font-size: 0.82rem;
        font-weight: 700;
        letter-spacing: 0.08em;
        padding: 0.3rem 0.8rem;
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

      .launcher-row-status {
        background: rgba(255, 255, 255, 0.03);
        border: 1px dashed rgba(255, 255, 255, 0.08);
      }

      .launcher-row-status:selected {
        background: rgba(255, 255, 255, 0.06);
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

#[cfg(test)]
mod tests {
    use super::default_ssh_terminal;
    use std::fs::{self, File};

    #[test]
    fn prefers_launch_kitty_wrapper_when_it_is_executable() {
        let temp_home = std::env::temp_dir().join(format!(
            "dot-launcher-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let wrapper = temp_home.join(".dotfiles/scripts/launch_kitty.sh");
        fs::create_dir_all(wrapper.parent().expect("wrapper parent")).expect("create wrapper dir");
        File::create(&wrapper).expect("create wrapper");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&wrapper)
                .expect("wrapper metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&wrapper, permissions).expect("set executable bit");
        }

        assert_eq!(
            default_ssh_terminal(Some(&temp_home)),
            wrapper.to_string_lossy()
        );

        fs::remove_dir_all(&temp_home).expect("cleanup temp home");
    }

    #[test]
    fn falls_back_to_kitty_without_wrapper() {
        assert_eq!(default_ssh_terminal(None), "kitty");
    }
}
