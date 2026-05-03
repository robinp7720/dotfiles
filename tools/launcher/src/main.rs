mod model;
mod prediction;
mod sources;

use crate::model::{Action, QueryInput, ResultItem, SearchMode};
use crate::sources::{Sources, focus_window};
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
use std::cell::Cell;
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "dot-launcher")]
#[command(
    about = "Unified predictive desktop launcher for apps, windows, files, passwords, SSH, commands, web, and libqalculate"
)]
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
    sources.warm_external_sources();
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
        .default_width(860)
        .default_height(560)
        .decorated(false)
        .resizable(false)
        .title("Launcher")
        .build();

    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Overlay);
    // The launcher should behave like a modal overlay. On-demand focus is
    // compositor-defined and can leave the entry without a working key grab.
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);
    window.set_namespace(Some("dot-launcher"));
    window.set_anchor(gtk4_layer_shell::Edge::Top, true);
    window.set_margin(gtk4_layer_shell::Edge::Top, 112);

    apply_css();

    let outer = GtkBox::new(Orientation::Vertical, 18);
    outer.add_css_class("launcher-shell");
    outer.set_halign(Align::Center);
    outer.set_size_request(860, -1);
    outer.set_margin_top(26);
    outer.set_margin_bottom(26);
    outer.set_margin_start(26);
    outer.set_margin_end(26);

    let header = GtkBox::new(Orientation::Vertical, 6);
    header.add_css_class("launcher-header");

    let kicker = Label::new(Some("Desktop Launchpad"));
    kicker.add_css_class("launcher-kicker");
    kicker.set_halign(Align::Start);

    let headline = Label::new(Some(
        "Apps, windows, files, passwords, hosts, commands, and quick math in one overlay",
    ));
    headline.add_css_class("launcher-headline");
    headline.set_halign(Align::Start);
    headline.set_wrap(true);

    header.append(&kicker);
    header.append(&headline);

    let entry = Entry::builder()
        .placeholder_text(placeholder_for_mode(mode))
        .build();
    entry.add_css_class("launcher-entry");
    entry.set_icon_from_icon_name(
        gtk4::EntryIconPosition::Primary,
        Some("system-search-symbolic"),
    );
    if let Some(query) = initial_query.as_deref() {
        entry.set_text(query);
    }

    let hint = Label::new(Some(
        "Prefixes: ~ windows, > commands, / files, @ ssh, ! pass, ? web, = calc",
    ));
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

    let shortcuts = GtkBox::new(Orientation::Horizontal, 8);
    shortcuts.add_css_class("launcher-shortcuts");
    shortcuts.set_halign(Align::Start);
    for chip in [
        "Applications",
        "~ Windows",
        "/ Files",
        "@ SSH",
        "! Pass",
        "> Commands",
        "? Web",
        "= Calc",
    ] {
        shortcuts.append(&build_shortcut_chip(chip));
    }

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.add_css_class("launcher-results");

    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .min_content_height(380)
        .child(&list)
        .build();
    scroller.add_css_class("launcher-scroller");

    outer.append(&header);
    outer.append(&entry);
    outer.append(&meta);
    outer.append(&shortcuts);
    outer.append(&scroller);
    window.set_child(Some(&outer));

    let current_results = Rc::new(RefCell::new(Vec::<ResultItem>::new()));

    {
        let sources = sources.clone();
        let list = list.clone();
        let scroller = scroller.clone();
        let hint = hint.clone();
        let mode_badge = mode_badge.clone();
        let current_results = current_results.clone();
        entry.connect_changed(move |entry| {
            let query = entry.text().to_string();
            update_search_meta(&hint, &mode_badge, &query, mode);
            let results = sources.search(&query, mode);
            rebuild_results(&list, &scroller, &results);
            current_results.replace(results);
        });
    }

    {
        let hint = hint.clone();
        let window = window.clone();
        let sources = sources.clone();
        let current_results = current_results.clone();
        list.connect_row_activated(move |_, row| {
            let results = current_results.borrow();
            activate_row(row, &results, &window, &hint, &sources);
        });
    }

    {
        let hint = hint.clone();
        let list = list.clone();
        let window = window.clone();
        let sources = sources.clone();
        let activate_entry = entry.clone();
        let current_results = current_results.clone();
        entry.connect_activate(move |_| {
            let query = activate_entry.text().to_string();
            let results = current_results.borrow();
            let selected = list
                .selected_row()
                .and_then(|row| results.get(row.index() as usize).cloned())
                .or_else(|| {
                    if query.is_empty() {
                        None
                    } else {
                        results.first().cloned()
                    }
                });

            if let Some(item) = selected {
                if let Err(error) = execute_action(&window, item.action.clone()) {
                    set_hint_state(
                        &hint,
                        &format!("Action failed: {}", error.root_cause()),
                        true,
                    );
                } else {
                    sources.record_activation(&item);
                }
            }
        });
    }

    {
        let entry = entry.clone();
        let list = list.clone();
        let scroller = scroller.clone();
        let current_results = current_results.clone();
        let keys = EventControllerKey::new();
        let key_window = window.clone();
        keys.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                key_window.close();
                glib::Propagation::Stop
            }
            gdk::Key::Down => {
                let result_count = current_results.borrow().len() as i32;
                move_selection(&list, &scroller, 1, result_count);
                entry.grab_focus();
                glib::Propagation::Stop
            }
            gdk::Key::Up => {
                let result_count = current_results.borrow().len() as i32;
                move_selection(&list, &scroller, -1, result_count);
                entry.grab_focus();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        window.add_controller(keys);
    }

    {
        let focus_armed = Rc::new(Cell::new(false));

        {
            let focus_armed = focus_armed.clone();
            let window = window.clone();
            entry.connect_has_focus_notify(move |entry| {
                if entry.has_focus() {
                    focus_armed.set(true);
                } else if focus_armed.get() && window.is_visible() {
                    window.close();
                }
            });
        }

        {
            let focus_armed = focus_armed.clone();
            window.connect_is_active_notify(move |window| {
                if focus_armed.get() && !window.is_active() && window.is_visible() {
                    window.close();
                }
            });
        }
    }

    let initial_results = sources.search(initial_query.as_deref().unwrap_or_default(), mode);
    update_search_meta(
        &hint,
        &mode_badge,
        initial_query.as_deref().unwrap_or_default(),
        mode,
    );
    rebuild_results(&list, &scroller, &initial_results);
    current_results.replace(initial_results);

    window.present();
    request_initial_focus(&window, &entry);
}

fn request_initial_focus(window: &ApplicationWindow, entry: &Entry) {
    for delay_ms in [0_u64, 25, 100, 250] {
        let window = window.clone();
        let entry = entry.clone();
        glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
            if !window.is_visible() || entry.has_focus() {
                return;
            }

            window.present();
            entry.grab_focus_without_selecting();
        });
    }
}

fn build_shortcut_chip(label: &str) -> Label {
    let chip = Label::new(Some(label));
    chip.add_css_class("launcher-shortcut-chip");
    chip
}

fn rebuild_results(list: &ListBox, scroller: &ScrolledWindow, results: &[ResultItem]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for item in results {
        let row = build_row(item);
        list.append(&row);
    }

    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
        scroll_row_into_view(list, scroller, &row);
    }
}

fn build_row(item: &ResultItem) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("launcher-row");
    if matches!(&item.action, Action::None) {
        row.add_css_class("launcher-row-status");
    }

    let layout = GtkBox::new(Orientation::Horizontal, 14);
    layout.set_margin_top(12);
    layout.set_margin_bottom(12);
    layout.set_margin_start(14);
    layout.set_margin_end(14);

    let icon = Image::from_icon_name(&item.icon_name);
    icon.set_pixel_size(28);
    icon.add_css_class("launcher-icon");
    icon.set_halign(Align::Center);
    icon.set_valign(Align::Center);

    let icon_wrap = GtkBox::new(Orientation::Vertical, 0);
    icon_wrap.add_css_class("launcher-icon-wrap");
    icon_wrap.set_valign(Align::Center);
    icon_wrap.set_halign(Align::Center);
    icon_wrap.append(&icon);

    let text_box = GtkBox::new(Orientation::Vertical, 4);
    text_box.set_hexpand(true);

    let title = Label::new(Some(&item.title));
    title.add_css_class("launcher-title");
    title.set_halign(Align::Start);
    title.set_xalign(0.0);
    title.set_wrap(true);

    let subtitle = Label::new(Some(&item.subtitle));
    subtitle.add_css_class("launcher-subtitle");
    subtitle.set_halign(Align::Start);
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);

    let source_badge = Label::new(Some(item.source));
    source_badge.add_css_class("launcher-source-badge");
    source_badge.set_valign(Align::Center);

    text_box.append(&title);
    text_box.append(&subtitle);

    layout.append(&icon_wrap);
    layout.append(&text_box);
    layout.append(&source_badge);
    row.set_child(Some(&layout));
    row
}

fn move_selection(list: &ListBox, scroller: &ScrolledWindow, delta: i32, result_count: i32) {
    if result_count <= 0 {
        return;
    }

    let current = list.selected_row().map(|row| row.index()).unwrap_or(0);
    let next = (current + delta).clamp(0, result_count - 1);
    if let Some(row) = list.row_at_index(next) {
        list.select_row(Some(&row));
        scroll_row_into_view(list, scroller, &row);
    }
}

fn scroll_row_into_view(list: &ListBox, scroller: &ScrolledWindow, row: &ListBoxRow) {
    let adjustment = scroller.vadjustment();
    let visible_top = adjustment.value();
    let visible_bottom = visible_top + adjustment.page_size();
    let Some(bounds) = row.compute_bounds(list) else {
        return;
    };
    let row_top = f64::from(bounds.y());
    let row_bottom = row_top + f64::from(bounds.height());

    let next_value = if row_top < visible_top {
        row_top
    } else if row_bottom > visible_bottom {
        row_bottom - adjustment.page_size()
    } else {
        return;
    };

    let max_value = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
    adjustment.set_value(next_value.clamp(adjustment.lower(), max_value));
}

fn activate_row(
    row: &ListBoxRow,
    results: &[ResultItem],
    window: &ApplicationWindow,
    hint: &Label,
    sources: &Sources,
) {
    let index = row.index() as usize;
    if let Some(item) = results.get(index).cloned() {
        if let Err(error) = execute_action(window, item.action.clone()) {
            set_hint_state(
                hint,
                &format!("Action failed: {}", error.root_cause()),
                true,
            );
        } else {
            sources.record_activation(&item);
        }
    }
}

fn execute_action(window: &ApplicationWindow, action: Action) -> Result<()> {
    match action {
        Action::LaunchApp { desktop_id } => launch_desktop_app(&desktop_id)?,
        Action::FocusWindow { target } => {
            let status = focus_window(&target).context("failed to focus selected window")?;
            if !status.success() {
                anyhow::bail!("window focus command failed");
            }
        }
        Action::OpenFile { path } => {
            let file = gio::File::for_path(path);
            gio::AppInfo::launch_default_for_uri(&file.uri(), gio::AppLaunchContext::NONE)
                .context("failed to open file")?;
        }
        Action::Ssh { host } => launch_ssh(&host)?,
        Action::CopyPass { entry } => {
            let secret = load_pass_secret(&entry)?;
            copy_to_clipboard(&secret);
        }
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
        Action::OpenUrl { url } => {
            gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE)
                .context("failed to open URL")?;
        }
        Action::CopyText { text } => {
            copy_to_clipboard(&text);
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
        SearchMode::All => "Search apps, files, passwords, SSH, commands, web, and calculations",
        SearchMode::Apps => "Launch an application",
        SearchMode::Windows => "Switch to an active window",
        SearchMode::Files => "Search files with LocalSearch",
        SearchMode::Ssh => "Search SSH hosts",
        SearchMode::Pass => "Search password-store entries",
        SearchMode::Commands => "Run a command",
        SearchMode::Web => "Search the web or open a URL",
        SearchMode::Calc => "Evaluate a libqalculate expression",
    }
}

fn update_search_meta(hint: &Label, mode_badge: &Label, raw_query: &str, cli_mode: SearchMode) {
    let query = QueryInput::parse(raw_query, cli_mode);
    mode_badge.set_text(query.mode.label());

    let hint_text = match query.mode {
        SearchMode::All => "Prefixes: ~ windows, > commands, / files, @ ssh, ! pass, ? web, = calc",
        SearchMode::Apps => "Search installed desktop applications",
        SearchMode::Windows => "Search active windows and press Enter to focus one",
        SearchMode::Files => "Search LocalSearch indexed files",
        SearchMode::Ssh => "Search aliases from ~/.ssh/config and known_hosts",
        SearchMode::Pass => "Press Enter to copy the first line from pass show",
        SearchMode::Commands => "Press Enter to run the command or choose a suggestion",
        SearchMode::Web => "Press Enter to search the web or open a URL directly",
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

fn copy_to_clipboard(text: &str) {
    if let Some(display) = gdk::Display::default() {
        display.clipboard().set_text(text);
    }
}

fn load_pass_secret(entry: &str) -> Result<String> {
    let output = Command::new("pass")
        .args(["show", entry])
        .output()
        .context("failed to run pass")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "{}",
            if stderr.is_empty() {
                "pass failed to decrypt the selected entry"
            } else {
                stderr.as_str()
            }
        );
    }

    String::from_utf8(output.stdout)
        .context("pass returned non-UTF-8 output")?
        .lines()
        .next()
        .map(|line| line.to_string())
        .filter(|line| !line.is_empty())
        .context("pass entry did not contain a password on the first line")
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
        background: linear-gradient(180deg, rgba(19, 23, 33, 0.78), rgba(12, 15, 24, 0.92));
        border: 1px solid rgba(255, 255, 255, 0.10);
        border-radius: 24px;
        box-shadow: 0 28px 80px rgba(0, 0, 0, 0.42);
        padding: 1.35rem;
      }

      .launcher-header {
        padding: 0.1rem 0.15rem 0.2rem;
      }

      .launcher-kicker {
        color: rgba(188, 214, 255, 0.72);
        font-size: 0.82rem;
        font-weight: 700;
        letter-spacing: 0.16em;
        text-transform: uppercase;
      }

      .launcher-headline {
        color: rgba(245, 248, 255, 0.98);
        font-size: 1.34rem;
        font-weight: 700;
      }

      .launcher-entry {
        min-height: 64px;
        font-size: 1.18rem;
        padding: 0.45rem 0.95rem;
        border-radius: 18px;
        border: 1px solid rgba(255, 255, 255, 0.09);
        background: rgba(255, 255, 255, 0.07);
        color: rgba(247, 249, 255, 0.98);
        box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
      }

      .launcher-entry:focus-within {
        border-color: rgba(142, 188, 255, 0.55);
        background: rgba(255, 255, 255, 0.10);
        box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08),
                    0 0 0 3px rgba(106, 160, 255, 0.14);
      }

      .launcher-hint {
        color: rgba(255, 255, 255, 0.68);
        font-size: 0.9rem;
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

      .launcher-shortcuts {
        margin-top: -0.1rem;
      }

      .launcher-shortcut-chip {
        background: rgba(255, 255, 255, 0.05);
        border: 1px solid rgba(255, 255, 255, 0.06);
        border-radius: 999px;
        color: rgba(233, 238, 248, 0.82);
        font-size: 0.83rem;
        font-weight: 600;
        padding: 0.3rem 0.72rem;
      }

      .launcher-results {
        background: transparent;
      }

      .launcher-row {
        margin-bottom: 8px;
        border-radius: 18px;
        border: 1px solid rgba(255, 255, 255, 0.02);
        background: rgba(255, 255, 255, 0.02);
      }

      .launcher-row:selected {
        background: linear-gradient(90deg, rgba(120, 168, 255, 0.16), rgba(255, 255, 255, 0.08));
        border-color: rgba(142, 188, 255, 0.22);
      }

      .launcher-row-status {
        background: rgba(255, 255, 255, 0.04);
        border: 1px dashed rgba(255, 255, 255, 0.08);
      }

      .launcher-row-status:selected {
        background: rgba(255, 255, 255, 0.07);
      }

      .launcher-icon-wrap {
        min-width: 44px;
        border-radius: 14px;
        background: rgba(255, 255, 255, 0.07);
        border: 1px solid rgba(255, 255, 255, 0.04);
        padding: 8px;
      }

      .launcher-icon {
        color: rgba(240, 244, 255, 0.96);
      }

      .launcher-title {
        font-size: 1.05rem;
        font-weight: 700;
      }

      .launcher-subtitle {
        color: rgba(255, 255, 255, 0.70);
        font-size: 0.92rem;
      }

      .launcher-source-badge {
        background: rgba(255, 255, 255, 0.06);
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 999px;
        color: rgba(233, 238, 248, 0.74);
        font-size: 0.76rem;
        font-weight: 700;
        letter-spacing: 0.06em;
        padding: 0.28rem 0.72rem;
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
