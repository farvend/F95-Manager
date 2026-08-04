use super::*;

fn download_visual(progress: Option<&Progress>) -> (DownloadVisualState, f32, String) {
    match progress {
        Some(Progress::Pending(value)) => (
            DownloadVisualState::Determinate,
            value.clamp(0.0, 1.0),
            String::new(),
        ),
        Some(Progress::Error(error)) => (DownloadVisualState::Error, 0.0, error.clone()),
        Some(Progress::Unknown) => (DownloadVisualState::Indeterminate, 0.0, String::new()),
        Some(Progress::Paused) => (DownloadVisualState::Paused, 0.0, String::new()),
        None => (DownloadVisualState::Idle, 0.0, String::new()),
    }
}

pub(super) fn watch_download(
    id: u64,
    receiver: std::sync::mpsc::Receiver<GameDownloadStatus>,
    state: SharedState,
    weak: slint::Weak<MainWindow>,
) {
    crate::app::rt().spawn_blocking(move || {
        while let Ok(status) = receiver.recv() {
            let mut completed = None;
            let mut reported_error = None;
            {
                let Ok(mut state) = state.lock() else { return };
                match status {
                    GameDownloadStatus::Downloading(progress) => {
                        if let Progress::Error(error) = &progress {
                            reported_error = Some(format!("Download error (thread {id}): {error}"));
                        }
                        if let Some(job) = state.downloads.get_mut(&id) {
                            job.progress = match progress {
                                Progress::Pending(value) => {
                                    Progress::Pending(value.clamp(0.0, 1.0) * 0.75)
                                }
                                other => other,
                            };
                        }
                    }
                    GameDownloadStatus::Unzipping(progress) => {
                        if let Progress::Error(error) = &progress {
                            reported_error = Some(format!("Unzip error (thread {id}): {error}"));
                        }
                        if let Some(job) = state.downloads.get_mut(&id) {
                            job.progress = match progress {
                                Progress::Pending(value) => {
                                    Progress::Pending(0.75 + value.clamp(0.0, 1.0) * 0.25)
                                }
                                other => other,
                            };
                        }
                    }
                    GameDownloadStatus::SelectLinks(links) => {
                        if links.is_empty() {
                            if let Some(job) = state.downloads.get_mut(&id) {
                                job.progress =
                                    Progress::Error("Download links not found".to_string());
                            }
                        } else if let Some(job) = state.downloads.get_mut(&id) {
                            job.progress = Progress::Unknown;
                            job.link_choices = links;
                        }
                    }
                    GameDownloadStatus::Completed { dest_dir, exe_path } => {
                        state.downloads.remove(&id);
                        if let Some(card) = state.cards.iter_mut().find(|card| card.id == id) {
                            card.installed = true;
                            card.folder = Some(dest_dir.clone());
                        }
                        completed = Some((dest_dir, exe_path));
                    }
                }
            }

            let was_completed = completed.is_some();
            if let Some(error) = reported_error {
                append_error(error, &weak);
            }
            if let Some((folder, exe_path)) = completed {
                crate::app::settings::record_downloaded_game(id, folder, exe_path);
            }
            let state_for_ui = state.clone();
            let _ = weak.upgrade_in_event_loop(move |ui| {
                update_card_download(&ui, &state_for_ui, id);
                show_download_links(&ui, &state_for_ui, id);
            });
            if was_completed {
                break;
            }
        }
    });
}

fn link_label(link: &DownloadLink) -> String {
    match link {
        DownloadLink::Direct(link) => {
            let path = link.path.join("/");
            if path.is_empty() {
                link.hosting.to_string()
            } else {
                format!("{}/{}", link.hosting, path)
            }
        }
        DownloadLink::Masked(url) => format!("{}{}", url.domain().unwrap_or_default(), url.path()),
    }
}

pub(super) fn show_download_links(ui: &MainWindow, state: &SharedState, id: u64) {
    let Ok(state) = state.lock() else { return };
    let Some(job) = state.downloads.get(&id) else {
        return;
    };
    if job.link_choices.is_empty() {
        return;
    }
    let links = job
        .link_choices
        .iter()
        .enumerate()
        .map(|(index, link)| SuggestionData {
            id: index.to_string().into(),
            label: link_label(link).into(),
        })
        .collect::<Vec<_>>();
    ui.set_download_link_game_id(id.to_string().into());
    ui.set_download_links(ModelRc::from(Rc::new(VecModel::from(links))));
    ui.set_download_link_visible(true);
}

pub(super) fn update_card_download(ui: &MainWindow, state: &SharedState, id: u64) {
    let Ok(state) = state.lock() else { return };
    let columns = state.columns.max(1);
    let Some(index) = state.cards.iter().position(|card| card.id == id) else {
        return;
    };
    let rows = ui.get_game_rows();
    let Some(row) = rows.row_data(index / columns) else {
        return;
    };
    let Some(mut card) = row.cards.row_data(index % columns) else {
        return;
    };
    let (download_state, download_progress, download_error) =
        download_visual(state.downloads.get(&id).map(|job| &job.progress));
    card.download_state = download_state;
    card.download_progress = download_progress;
    card.download_error = download_error.into();
    if let Some(record) = state.cards.get(index) {
        card.installed = record.installed;
    }
    row.cards.set_row_data(index % columns, card);
}

pub(super) fn card_download_visual(state: &UiState, id: u64) -> (DownloadVisualState, f32, String) {
    download_visual(state.downloads.get(&id).map(|job| &job.progress))
}
