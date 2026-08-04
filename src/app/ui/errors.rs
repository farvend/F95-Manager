use super::*;

static ERRORS: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

fn errors() -> &'static Mutex<VecDeque<String>> {
    ERRORS.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub(super) fn append_error(message: impl Into<String>, weak: &slint::Weak<MainWindow>) {
    let message = message.into();
    log::error!("{message}");
    let count = if let Ok(mut errors) = errors().lock() {
        errors.push_back(message);
        while errors.len() > 1000 {
            errors.pop_front();
        }
        errors.len()
    } else {
        0
    };
    let _ = weak
        .clone()
        .upgrade_in_event_loop(move |ui| ui.set_error_count(count as i32));
}

pub(super) fn refresh_errors(window: &ErrorsWindow) {
    let (contents, count) = errors()
        .lock()
        .map(|errors| {
            (
                errors.iter().cloned().collect::<Vec<_>>().join("\n"),
                errors.len(),
            )
        })
        .unwrap_or_default();
    window.set_contents(contents.into());
    window.set_error_count(count as i32);
}

pub(super) fn clear_errors(window: &ErrorsWindow, main: &slint::Weak<MainWindow>) {
    if let Ok(mut errors) = errors().lock() {
        errors.clear();
    }
    refresh_errors(window);
    if let Some(main) = main.upgrade() {
        main.set_error_count(0);
    }
}

pub(super) fn copy_errors() {
    let text = errors()
        .lock()
        .map(|errors| errors.iter().cloned().collect::<Vec<_>>().join("\n"))
        .unwrap_or_default();
    if let Err(error) = clipboard_win::set_clipboard_string(&text) {
        log::warn!("Failed to copy errors: {error}");
    }
}
