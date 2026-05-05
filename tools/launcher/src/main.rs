mod model;
mod password;
mod prediction;
mod sources;

use crate::model::{Action, ResultItem, SearchMode};
use crate::model::{PasswordOperation, WindowFocusTarget};
use crate::password::{
    Credential, TypeStep, default_login_steps, parse_credential, run_program_input,
    wl_copy_command, wtype_commands_for_steps, xclip_command, xdotool_commands_for_steps,
};
use crate::sources::{Sources, focus_window, focused_window_target};
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
use std::thread;
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

const LAUNCHER_WIDTH_PX: i32 = 720;
const LAUNCHER_LAYER_TOP_MARGIN_PX: i32 = 72;
const LAUNCHER_SURFACE_MARGIN_PX: i32 = 56;
const LAUNCHER_SHADOW_Y_OFFSET_PX: i32 = 18;
const LAUNCHER_SHADOW_BLUR_PX: i32 = 44;
const LAUNCHER_SURFACE_MARGIN_BOTTOM_PX: i32 =
    LAUNCHER_SHADOW_BLUR_PX + LAUNCHER_SHADOW_Y_OFFSET_PX + 8;

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
        .default_width(LAUNCHER_WIDTH_PX)
        .default_height(420)
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
    window.set_margin(gtk4_layer_shell::Edge::Top, LAUNCHER_LAYER_TOP_MARGIN_PX);

    apply_css();
    let previous_focus_target = Rc::new(focused_window_target());

    let outer = GtkBox::new(Orientation::Vertical, 10);
    outer.add_css_class("launcher-shell");
    outer.set_halign(Align::Center);
    outer.set_size_request(LAUNCHER_WIDTH_PX, -1);
    outer.set_margin_top(LAUNCHER_SURFACE_MARGIN_PX);
    outer.set_margin_bottom(LAUNCHER_SURFACE_MARGIN_BOTTOM_PX);
    outer.set_margin_start(LAUNCHER_SURFACE_MARGIN_PX);
    outer.set_margin_end(LAUNCHER_SURFACE_MARGIN_PX);

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

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.add_css_class("launcher-results");

    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .min_content_height(300)
        .child(&list)
        .build();
    scroller.add_css_class("launcher-scroller");

    outer.append(&entry);
    outer.append(&scroller);
    window.set_child(Some(&outer));

    let current_results = Rc::new(RefCell::new(Vec::<ResultItem>::new()));

    {
        let sources = sources.clone();
        let list = list.clone();
        let scroller = scroller.clone();
        let current_results = current_results.clone();
        entry.connect_changed(move |entry| {
            let query = entry.text().to_string();
            let results = sources.search(&query, mode);
            rebuild_results(&list, &scroller, &results);
            current_results.replace(results);
        });
    }

    {
        let list = list.clone();
        let status_list = list.clone();
        let scroller = scroller.clone();
        let window = window.clone();
        let sources = sources.clone();
        let current_results = current_results.clone();
        let previous_focus_target = previous_focus_target.clone();
        list.connect_row_activated(move |_, row| {
            let item = {
                let results = current_results.borrow();
                results.get(row.index() as usize).cloned()
            };
            if let Some(item) = item {
                activate_item(
                    &window,
                    &sources,
                    item,
                    &status_list,
                    &scroller,
                    &current_results,
                    previous_focus_target.as_ref().as_ref(),
                );
            }
        });
    }

    {
        let list = list.clone();
        let status_list = list.clone();
        let scroller = scroller.clone();
        let window = window.clone();
        let sources = sources.clone();
        let activate_entry = entry.clone();
        let current_results = current_results.clone();
        let previous_focus_target = previous_focus_target.clone();
        entry.connect_activate(move |_| {
            let query = activate_entry.text().to_string();
            let selected = {
                let results = current_results.borrow();
                list.selected_row()
                    .and_then(|row| results.get(row.index() as usize).cloned())
                    .or_else(|| {
                        if query.is_empty() {
                            None
                        } else {
                            results.first().cloned()
                        }
                    })
            };

            if let Some(item) = selected {
                activate_item(
                    &window,
                    &sources,
                    item,
                    &status_list,
                    &scroller,
                    &current_results,
                    previous_focus_target.as_ref().as_ref(),
                );
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
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(10);
    layout.set_margin_end(10);

    let icon = Image::from_icon_name(&item.icon_name);
    icon.set_pixel_size(24);
    icon.add_css_class("launcher-icon");
    icon.set_halign(Align::Center);
    icon.set_valign(Align::Center);

    let icon_wrap = GtkBox::new(Orientation::Vertical, 0);
    icon_wrap.add_css_class("launcher-icon-wrap");
    icon_wrap.set_valign(Align::Center);
    icon_wrap.set_halign(Align::Center);
    icon_wrap.append(&icon);

    let title = Label::new(Some(&item.title));
    title.add_css_class("launcher-title");
    title.set_halign(Align::Start);
    title.set_hexpand(true);
    title.set_xalign(0.0);
    title.set_wrap(false);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    if let Some(tooltip) = row_tooltip_text(item) {
        row.set_tooltip_text(Some(&tooltip));
    }

    layout.append(&icon_wrap);
    layout.append(&title);
    row.set_child(Some(&layout));
    row
}

fn row_tooltip_text(item: &ResultItem) -> Option<String> {
    let subtitle = item.subtitle.trim();
    let source = item.source.trim();

    match (subtitle.is_empty(), source.is_empty()) {
        (true, true) => None,
        (false, true) => Some(subtitle.to_string()),
        (true, false) => Some(source.to_string()),
        (false, false) => Some(format!("{subtitle}\n{source}")),
    }
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

fn activate_item(
    window: &ApplicationWindow,
    sources: &Sources,
    item: ResultItem,
    list: &ListBox,
    scroller: &ScrolledWindow,
    current_results: &Rc<RefCell<Vec<ResultItem>>>,
    previous_focus_target: Option<&WindowFocusTarget>,
) {
    if let Action::Password {
        entry,
        operation: PasswordOperation::Inspect,
    } = &item.action
    {
        match load_pass_credential(entry) {
            Ok(credential) => {
                let results = inspected_password_results(&credential);
                rebuild_results(list, scroller, &results);
                current_results.replace(results);
            }
            Err(error) => show_status_result(
                list,
                scroller,
                current_results,
                action_failure_result(&error.root_cause().to_string()),
            ),
        }
        return;
    }

    if let Err(error) = execute_action(window, item.action.clone(), previous_focus_target) {
        show_status_result(
            list,
            scroller,
            current_results,
            action_failure_result(&error.root_cause().to_string()),
        );
    } else {
        sources.record_activation(&item);
    }
}

fn show_status_result(
    list: &ListBox,
    scroller: &ScrolledWindow,
    current_results: &Rc<RefCell<Vec<ResultItem>>>,
    item: ResultItem,
) {
    let results = vec![item];
    rebuild_results(list, scroller, &results);
    current_results.replace(results);
}

fn action_failure_result(message: &str) -> ResultItem {
    ResultItem {
        prediction_key: None,
        title: format!("Action failed: {message}"),
        subtitle: String::new(),
        source: "Status",
        icon_name: "dialog-error-symbolic".to_string(),
        score: 0,
        action: Action::None,
    }
}

fn execute_action(
    window: &ApplicationWindow,
    action: Action,
    previous_focus_target: Option<&WindowFocusTarget>,
) -> Result<()> {
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
            copy_secret(&secret)?;
            window.close();
            return Ok(());
        }
        Action::Password { entry, operation } => {
            execute_password_operation(window, &entry, operation, previous_focus_target)?;
            return Ok(());
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

fn execute_password_operation(
    window: &ApplicationWindow,
    entry: &str,
    operation: PasswordOperation,
    previous_focus_target: Option<&WindowFocusTarget>,
) -> Result<()> {
    let credential = load_pass_credential(entry)?;

    match operation {
        PasswordOperation::AutotypeLogin => {
            type_secret_steps(
                window,
                previous_focus_target,
                default_login_steps(&credential),
            )?;
        }
        PasswordOperation::CopyPassword => {
            copy_secret(&credential.password)?;
            window.close();
        }
        PasswordOperation::CopyUsername => {
            copy_secret(&credential.username)?;
            window.close();
        }
        PasswordOperation::TypePassword => {
            type_secret_steps(
                window,
                previous_focus_target,
                vec![TypeStep::Text(credential.password)],
            )?;
        }
        PasswordOperation::TypeUsername => {
            type_secret_steps(
                window,
                previous_focus_target,
                vec![TypeStep::Text(credential.username)],
            )?;
        }
        PasswordOperation::OpenUrl => {
            let url = credential
                .url
                .context("pass entry does not contain a URL")?;
            gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE)
                .context("failed to open URL")?;
            window.close();
        }
        PasswordOperation::CopyUrl => {
            let url = credential
                .url
                .context("pass entry does not contain a URL")?;
            copy_secret(&url)?;
            window.close();
        }
        PasswordOperation::CopyOtp => {
            let otp = load_pass_otp(entry)?;
            copy_secret(&otp)?;
            window.close();
        }
        PasswordOperation::TypeOtp => {
            let otp = load_pass_otp(entry)?;
            type_secret_steps(window, previous_focus_target, vec![TypeStep::Text(otp)])?;
        }
        PasswordOperation::CustomAutotype => {
            let steps = custom_autotype_steps(entry, &credential)?;
            type_secret_steps(window, previous_focus_target, steps)?;
        }
        PasswordOperation::Inspect => unreachable!("inspect is handled before action execution"),
    }

    Ok(())
}

fn type_secret_steps(
    window: &ApplicationWindow,
    previous_focus_target: Option<&WindowFocusTarget>,
    steps: Vec<TypeStep>,
) -> Result<()> {
    let target = previous_focus_target.context("no previously focused window was captured")?;
    if !type_backend_available() {
        anyhow::bail!("wtype or xdotool is required for password autotype");
    }

    let status = focus_window(target).context("failed to refocus previous window")?;
    if !status.success() {
        anyhow::bail!("failed to refocus previous window");
    }

    window.close();
    thread::sleep(Duration::from_millis(160));

    let commands = if wayland_available() && command_exists("wtype") {
        wtype_commands_for_steps(&steps)
    } else {
        xdotool_commands_for_steps(&steps)
    };

    for command in commands {
        run_program_input(command)?;
    }

    Ok(())
}

fn type_backend_available() -> bool {
    (wayland_available() && command_exists("wtype")) || command_exists("xdotool")
}

fn copy_secret(text: &str) -> Result<()> {
    if wayland_available() && command_exists("wl-copy") {
        run_program_input(wl_copy_command(text, password_clip_timeout_seconds()))
    } else if command_exists("xclip") {
        run_program_input(xclip_command(text))?;
        clear_xclip_clipboard_after(password_clip_timeout_seconds());
        Ok(())
    } else {
        anyhow::bail!("wl-copy or xclip is required to copy password data");
    }
}

fn clear_xclip_clipboard_after(timeout_seconds: u64) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(timeout_seconds));
        let _ = Command::new("xclip")
            .args(["-selection", "clipboard", "-in"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(stdin) = child.stdin.take() {
                    drop(stdin);
                }
                child.wait().map(|_| ())
            });
    });
}

fn wayland_available() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn command_exists(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let path = dir.join(program);
            path.is_file() && is_executable(&path)
        })
    })
}

fn password_clip_timeout_seconds() -> u64 {
    std::env::var("PASSWORD_STORE_CLIP_TIME")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(15)
}

fn inspected_password_results(credential: &Credential) -> Vec<ResultItem> {
    let mut rows = vec![
        password_action_result(
            &credential.entry,
            "Autotype login",
            "Type username, Tab, and password without submitting",
            PasswordOperation::AutotypeLogin,
            1_000,
        ),
        password_action_result(
            &credential.entry,
            "Copy password",
            "Copy password and clear it after the password-store timeout",
            PasswordOperation::CopyPassword,
            950,
        ),
        password_action_result(
            &credential.entry,
            "Copy username",
            "Copy username metadata or the entry basename",
            PasswordOperation::CopyUsername,
            940,
        ),
        password_action_result(
            &credential.entry,
            "Type password",
            "Type only the password into the focused window",
            PasswordOperation::TypePassword,
            930,
        ),
        password_action_result(
            &credential.entry,
            "Type username",
            "Type only the username into the focused window",
            PasswordOperation::TypeUsername,
            920,
        ),
    ];

    if credential.url.is_some() {
        rows.push(password_action_result(
            &credential.entry,
            "Open URL",
            "Open this entry's URL in the default browser",
            PasswordOperation::OpenUrl,
            910,
        ));
        rows.push(password_action_result(
            &credential.entry,
            "Copy URL",
            "Copy this entry's URL",
            PasswordOperation::CopyUrl,
            900,
        ));
    }

    if credential.otp_uri.is_some() {
        rows.push(password_action_result(
            &credential.entry,
            "Copy OTP",
            "Generate and copy a one-time password with pass-otp",
            PasswordOperation::CopyOtp,
            890,
        ));
        rows.push(password_action_result(
            &credential.entry,
            "Type OTP",
            "Generate and type a one-time password with pass-otp",
            PasswordOperation::TypeOtp,
            880,
        ));
    }

    if credential.autotype.is_some() {
        rows.push(password_action_result(
            &credential.entry,
            "Custom autotype",
            "Run this entry's autotype template",
            PasswordOperation::CustomAutotype,
            870,
        ));
    }

    rows
}

fn password_action_result(
    entry: &str,
    title: &str,
    subtitle: &str,
    operation: PasswordOperation,
    score: i32,
) -> ResultItem {
    ResultItem {
        prediction_key: None,
        title: format!("{title}: {entry}"),
        subtitle: subtitle.to_string(),
        source: "Passwords",
        icon_name: "dialog-password-symbolic".to_string(),
        score,
        action: Action::Password {
            entry: entry.to_string(),
            operation,
        },
    }
}

fn custom_autotype_steps(entry: &str, credential: &Credential) -> Result<Vec<TypeStep>> {
    let template = credential
        .autotype
        .as_deref()
        .context("pass entry does not contain an autotype template")?;
    let mut steps = Vec::new();

    let mut tokens = template.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        match token {
            ":tab" => steps.push(TypeStep::Key("Tab")),
            ":space" => steps.push(TypeStep::Text(" ".to_string())),
            ":enter" => steps.push(TypeStep::Key("Return")),
            ":delay" => steps.push(TypeStep::Delay(1_000)),
            "pass" | "password" => steps.push(TypeStep::Text(credential.password.clone())),
            "user" | "username" => steps.push(TypeStep::Text(credential.username.clone())),
            "path" => steps.push(TypeStep::Text(entry.to_string())),
            "basename" | "filename" => {
                steps.push(TypeStep::Text(crate::password::fallback_username(entry)));
            }
            ":otp" => {
                if matches!(tokens.peek(), Some(&"pass") | Some(&"gopass")) {
                    tokens.next();
                }
                steps.push(TypeStep::Text(load_pass_otp(entry)?));
            }
            key => {
                let Some(value) = credential.fields.get(&key.to_ascii_lowercase()) else {
                    anyhow::bail!("unknown autotype token: {key}");
                };
                steps.push(TypeStep::Text(value.clone()));
            }
        }
    }

    Ok(steps)
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

fn copy_to_clipboard(text: &str) {
    if let Some(display) = gdk::Display::default() {
        display.clipboard().set_text(text);
    }
}

fn load_pass_secret(entry: &str) -> Result<String> {
    parse_credential(entry, &load_pass_output(&["show", entry])?)
        .map(|credential| credential.password)
}

fn load_pass_credential(entry: &str) -> Result<Credential> {
    parse_credential(entry, &load_pass_output(&["show", entry])?)
}

fn load_pass_otp(entry: &str) -> Result<String> {
    load_pass_output(&["otp", entry]).map(|output| output.trim().to_string())
}

fn load_pass_output(args: &[&str]) -> Result<String> {
    let output = Command::new("pass")
        .args(args)
        .output()
        .context("failed to run pass")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "{}",
            if stderr.is_empty() {
                "pass failed"
            } else {
                stderr.as_str()
            }
        );
    }

    String::from_utf8(output.stdout).context("pass returned non-UTF-8 output")
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

fn launcher_css() -> &'static str {
    r#"
      window {
        background: transparent;
      }

      .launcher-shell {
        background: linear-gradient(180deg, rgba(19, 23, 33, 0.78), rgba(12, 15, 24, 0.92));
        border: 1px solid rgba(255, 255, 255, 0.10);
        border-radius: 18px;
        box-shadow: 0 18px 44px rgba(0, 0, 0, 0.32);
        padding: 0.8rem;
      }

      .launcher-entry {
        min-height: 54px;
        font-size: 1.08rem;
        padding: 0.35rem 0.82rem;
        border-radius: 12px;
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

      .launcher-results {
        background: transparent;
      }

      .launcher-row {
        margin-bottom: 5px;
        border-radius: 12px;
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
        min-width: 34px;
        border-radius: 10px;
        background: rgba(255, 255, 255, 0.07);
        border: 1px solid rgba(255, 255, 255, 0.04);
        padding: 6px;
      }

      .launcher-icon {
        color: rgba(240, 244, 255, 0.96);
      }

      .launcher-title {
        font-size: 1rem;
        font-weight: 650;
      }
    "#
}

fn apply_css() {
    let css = launcher_css();
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
    use super::{
        LAUNCHER_SHADOW_BLUR_PX, LAUNCHER_SHADOW_Y_OFFSET_PX, LAUNCHER_SURFACE_MARGIN_BOTTOM_PX,
        LAUNCHER_SURFACE_MARGIN_PX, action_failure_result, default_ssh_terminal,
        inspected_password_results, launcher_css, row_tooltip_text,
    };
    use crate::model::{Action, PasswordOperation, ResultItem};
    use crate::password::parse_credential;
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

    #[test]
    fn action_failures_render_as_status_results() {
        let item = action_failure_result("permission denied");

        assert_eq!(item.title, "Action failed: permission denied");
        assert!(matches!(item.action, Action::None));
        assert_eq!(item.source, "Status");
        assert!(item.subtitle.is_empty());
    }

    #[test]
    fn row_tooltip_preserves_hidden_result_details() {
        let item = ResultItem {
            prediction_key: None,
            title: "Firefox".to_string(),
            subtitle: "Web Browser".to_string(),
            source: "Applications",
            icon_name: "firefox".to_string(),
            score: 100,
            action: Action::None,
        };

        assert_eq!(
            row_tooltip_text(&item).as_deref(),
            Some("Web Browser\nApplications")
        );
    }

    #[test]
    fn launcher_surface_reserves_room_for_the_css_shadow() {
        assert!(LAUNCHER_SURFACE_MARGIN_PX >= LAUNCHER_SHADOW_BLUR_PX);
        assert!(
            LAUNCHER_SURFACE_MARGIN_BOTTOM_PX
                >= LAUNCHER_SHADOW_BLUR_PX + LAUNCHER_SHADOW_Y_OFFSET_PX
        );
        assert!(launcher_css().contains("box-shadow: 0 18px 44px rgba(0, 0, 0, 0.32);"));
    }

    #[test]
    fn inspected_password_results_include_metadata_specific_actions() {
        let credential = parse_credential(
            "github/work",
            "secret\nuser: robin\nurl: https://github.com\notpauth://totp/GitHub:robin?secret=ABC\nautotype: user :tab pass\n",
        )
        .expect("credential");

        let operations = inspected_password_results(&credential)
            .into_iter()
            .filter_map(|item| match item.action {
                Action::Password { operation, .. } => Some(operation),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(operations.contains(&PasswordOperation::OpenUrl));
        assert!(operations.contains(&PasswordOperation::CopyUrl));
        assert!(operations.contains(&PasswordOperation::CopyOtp));
        assert!(operations.contains(&PasswordOperation::TypeOtp));
        assert!(operations.contains(&PasswordOperation::CustomAutotype));
    }
}
