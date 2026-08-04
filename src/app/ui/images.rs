use super::*;

pub(super) fn request_all_covers(state: SharedState, weak: slint::Weak<MainWindow>) {
    let ids = state
        .lock()
        .map(|state| state.cards.iter().map(|card| card.id).collect::<Vec<_>>())
        .unwrap_or_default();
    for id in ids {
        request_cover(id, state.clone(), weak.clone());
    }
}

pub(super) fn preload_library_data(state: SharedState, weak: slint::Weak<MainWindow>) {
    let (ids, cache_dir) = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            let mut seen = HashSet::new();
            let mut ids = settings
                .downloaded_games
                .iter()
                .filter(|game| seen.insert(game.thread_id))
                .map(|game| game.thread_id)
                .collect::<Vec<_>>();
            ids.extend(
                settings
                    .pending_downloads
                    .iter()
                    .copied()
                    .filter(|id| seen.insert(*id)),
            );
            (ids, settings.cache_dir.clone())
        })
        .unwrap_or_default();
    let target_size = cover_target_size();

    for id in ids {
        let load_image = state
            .lock()
            .map(|mut state| !state.loaded_images.contains(&id) && state.loading_images.insert(id))
            .unwrap_or(false);
        let state_for_task = state.clone();
        let weak_for_task = weak.clone();
        let cache_dir = cache_dir.clone();
        crate::app::rt().spawn(async move {
            let Ok(_permit) = library_preload_semaphore().acquire().await else {
                return;
            };
            let cache_dir_for_read = cache_dir.clone();
            let mut thread = tokio::task::spawn_blocking(move || {
                crate::app::fetch_helpers::load_from_cache(&cache_dir_for_read, id)
            })
            .await
            .ok()
            .flatten();
            if thread
                .as_ref()
                .is_none_or(crate::app::fetch_helpers::needs_enrich)
            {
                if let Ok(meta) = crate::parser::game_info::thread_meta::fetch_thread_meta(id).await
                {
                    let mut value = thread.unwrap_or_else(|| F95Thread {
                        thread_id: crate::parser::game_info::ThreadId(id),
                        title: format!("Thread #{id}"),
                        creator: String::new(),
                        version: String::new(),
                        cover: String::new(),
                        screens: Vec::new(),
                        tags: Vec::new(),
                        prefixes: Vec::new(),
                        date: String::new(),
                        views: 0,
                        likes: 0,
                        rating: 0.0,
                        watched: false,
                        ignored: false,
                        is_new: false,
                        ts: 0,
                    });
                    crate::app::fetch_helpers::apply_meta(&mut value, meta);
                    let cache_dir_for_write = cache_dir.clone();
                    let value_for_write = value.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        crate::app::fetch_helpers::save_to_cache(
                            &cache_dir_for_write,
                            id,
                            &value_for_write,
                        )
                    })
                    .await;
                    thread = Some(value);
                }
            }
            let cover_path = cache_dir.join(id.to_string()).join("cover.png");
            let decoded = if load_image {
                if cover_path.is_file() {
                    let path = cover_path.clone();
                    tokio::task::spawn_blocking(move || decode_image_path(path, target_size))
                        .await
                        .ok()
                        .flatten()
                } else if let Some(url) = thread
                    .as_ref()
                    .map(|thread| thread.cover.as_str())
                    .filter(|url| !url.trim().is_empty())
                {
                    match crate::parser::fetch_image_f95(url).await {
                        Ok((width, height, rgba)) => {
                            let pixels =
                                resize_cover_pixels(width as u32, height as u32, rgba, target_size);
                            if let Some(pixels) = pixels.as_ref() {
                                save_image_pixels(&cover_path, pixels);
                            }
                            pixels
                        }
                        Err(error) => {
                            log::warn!("Library cover preload failed for {id}: {error}");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(thread) = thread.as_ref() {
                preload_library_screens(id, thread.screens.clone(), cache_dir.clone(), target_size);
            }
            let state_for_ui = state_for_task.clone();
            let weak_for_refresh = weak_for_task.clone();
            let _ = weak_for_task.upgrade_in_event_loop(move |ui| {
                let mut schedule_refresh = false;
                if let Some(thread) = thread {
                    if let Ok(mut state) = state_for_ui.lock() {
                        if state.library_mode {
                            if let Some(card) = state.cards.iter_mut().find(|card| card.id == id) {
                                let folder = card.folder.clone();
                                let installed = card.installed;
                                *card = card_from_thread(
                                    thread,
                                    installed,
                                    folder,
                                    cover_path.is_file().then_some(cover_path.clone()),
                                );
                            }
                            if !state.library_refresh_scheduled {
                                state.library_refresh_scheduled = true;
                                schedule_refresh = true;
                            }
                        }
                    }
                }
                if load_image {
                    finish_cover_decode(&ui, &state_for_ui, id, decoded);
                }
                if schedule_refresh {
                    let state = state_for_ui.clone();
                    let weak = weak_for_refresh.clone();
                    slint::Timer::single_shot(std::time::Duration::from_millis(300), move || {
                        let should_refresh = state
                            .lock()
                            .map(|mut state| {
                                state.library_refresh_scheduled = false;
                                state.library_mode
                            })
                            .unwrap_or(false);
                        if should_refresh {
                            load_library(state, weak);
                        }
                    });
                }
            });
        });
    }
}

fn save_image_pixels(path: &std::path::Path, pixels: &ImagePixels) {
    let Some(parent) = path.parent() else { return };
    if let Err(error) = std::fs::create_dir_all(parent).and_then(|_| {
        image::save_buffer_with_format(
            path,
            &pixels.rgba,
            pixels.width,
            pixels.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(std::io::Error::other)
    }) {
        log::warn!("Failed to cache image {}: {error}", path.display());
    }
}

fn screen_cache_locks() -> &'static Mutex<HashMap<PathBuf, std::sync::Weak<tokio::sync::Mutex<()>>>>
{
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, std::sync::Weak<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn lock_screen_cache_file(
    path: &std::path::Path,
) -> Option<tokio::sync::OwnedMutexGuard<()>> {
    let lock = {
        let mut locks = screen_cache_locks().lock().ok()?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(path).and_then(std::sync::Weak::upgrade) {
            lock
        } else {
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
            lock
        }
    };
    Some(lock.lock_owned().await)
}

fn screen_preload_semaphore() -> &'static Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(8)))
}

fn preload_library_screens(
    id: u64,
    screens: Vec<String>,
    cache_dir: PathBuf,
    target_size: (u32, u32),
) {
    if screens.is_empty() {
        return;
    }
    crate::app::rt().spawn(async move {
        use futures::StreamExt;

        futures::stream::iter(screens.into_iter().enumerate())
            .for_each_concurrent(4, |(index, url)| {
                let cache_dir = cache_dir.clone();
                async move {
                    let Ok(_permit) = screen_preload_semaphore().acquire().await else {
                        return;
                    };
                    let cached = cache_dir
                        .join(id.to_string())
                        .join(format!("screen_{}.png", index + 1));
                    let Some(_file_guard) = lock_screen_cache_file(&cached).await else {
                        return;
                    };
                    if cached.is_file() {
                        return;
                    }
                    let url = crate::parser::normalize_url(&url);
                    let pixels = crate::parser::fetch_image_f95(&url).await.ok().and_then(
                        |(width, height, rgba)| {
                            resize_cover_pixels(width as u32, height as u32, rgba, target_size)
                        },
                    );
                    if let Some(pixels) = pixels.as_ref() {
                        save_image_pixels(&cached, pixels);
                    }
                }
            })
            .await;
    });
}

pub(super) fn request_cover(id: u64, state: SharedState, weak: slint::Weak<MainWindow>) {
    let source = {
        let Ok(mut state) = state.lock() else { return };
        if state.loaded_images.contains(&id) || !state.loading_images.insert(id) {
            return;
        }
        let source = state
            .cards
            .iter()
            .find(|card| card.id == id)
            .map(|card| (card.cached_cover.clone(), card.cover_url.clone()));
        if source.is_none() {
            state.loading_images.remove(&id);
        }
        source
    };
    let Some((cached, remote)) = source else {
        return;
    };
    let target_size = cover_target_size();
    let cache_path = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.cache_dir.join(id.to_string()).join("cover.png"))
        .unwrap_or_else(|_| {
            PathBuf::from("cache")
                .join(id.to_string())
                .join("cover.png")
        });
    crate::app::rt().spawn(async move {
        let Ok(_permit) = cover_semaphore().acquire().await else {
            return;
        };
        // Catalog cards only carry the remote URL. Still prefer a cover that a
        // previous run has already written to the common on-disk cache.
        let cached = cached
            .filter(|path| path.is_file())
            .or_else(|| cache_path.is_file().then(|| cache_path.clone()));
        let mut decoded = if let Some(path) = cached {
            tokio::task::spawn_blocking(move || decode_image_path(path, target_size))
                .await
                .ok()
                .flatten()
        } else {
            None
        };
        // A missing or damaged cache entry is the only reason to use the
        // network. Successful downloads replace the bad entry.
        if decoded.is_none()
            && let Some(url) = remote
        {
            let pixels = crate::parser::fetch_image_f95(&url).await.ok().and_then(
                |(width, height, rgba)| {
                    resize_cover_pixels(width as u32, height as u32, rgba, target_size)
                },
            );
            if let Some(pixels) = pixels.as_ref() {
                save_image_pixels(&cache_path, pixels);
            }
            decoded = pixels;
        }
        let state_for_ui = state.clone();
        let _ = weak
            .upgrade_in_event_loop(move |ui| finish_cover_decode(&ui, &state_for_ui, id, decoded));
    });
}

pub(super) fn cover_semaphore() -> &'static Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(2)))
}

pub(super) fn library_preload_semaphore() -> &'static Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(8)))
}

pub(super) fn request_screens(id: u64, state: SharedState, weak: slint::Weak<MainWindow>) {
    let screens = {
        let Ok(mut state) = state.lock() else { return };
        if state.loaded_screens.contains(&id) || !state.loading_screens.insert(id) {
            return;
        }
        let loaded = state.loaded_screen_images.clone();
        state
            .cards
            .iter()
            .find(|card| card.id == id)
            .map(|card| {
                card.screens
                    .iter()
                    .cloned()
                    .enumerate()
                    .filter(|(index, _)| !loaded.contains(&(id, *index)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    if screens.is_empty() {
        if let Ok(mut state) = state.lock() {
            state.loading_screens.remove(&id);
            state.loaded_screens.insert(id);
        }
        return;
    }
    let target_size = cover_target_size();
    let cache_dir = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.cache_dir.clone())
        .unwrap_or_else(|_| PathBuf::from("cache"));
    crate::app::rt().spawn(async move {
        let mut failed = false;
        for (index, url) in screens {
            let cached = cache_dir
                .join(id.to_string())
                .join(format!("screen_{}.png", index + 1));
            let Some(_file_guard) = lock_screen_cache_file(&cached).await else {
                failed = true;
                continue;
            };
            let decoded = if cached.is_file() {
                tokio::task::spawn_blocking(move || decode_image_path(cached, target_size))
                    .await
                    .ok()
                    .flatten()
            } else {
                let pixels = crate::parser::fetch_image_f95(&url).await.ok().and_then(
                    |(width, height, rgba)| {
                        resize_cover_pixels(width as u32, height as u32, rgba, target_size)
                    },
                );
                if let Some(pixels) = pixels.as_ref() {
                    save_image_pixels(&cached, pixels);
                }
                pixels
            };
            if let Some(decoded) = decoded {
                let state_for_ui = state.clone();
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    let evicted = cache_screen_image(id, index, &decoded);
                    if let Ok(mut state) = state_for_ui.lock() {
                        state.loaded_screen_images.insert((id, index));
                        for evicted_id in &evicted {
                            state
                                .loaded_screen_images
                                .retain(|(screen_id, _)| screen_id != evicted_id);
                            state.loaded_screens.remove(evicted_id);
                        }
                    }
                    for evicted_id in evicted {
                        update_card_image(&ui, &state_for_ui, evicted_id);
                    }
                    update_card_image(&ui, &state_for_ui, id);
                });
            } else {
                failed = true;
            }
        }
        if let Ok(mut state) = state.lock() {
            state.loading_screens.remove(&id);
            if !failed {
                state.loaded_screens.insert(id);
            }
        }
        let state_for_ui = state.clone();
        let _ = weak.upgrade_in_event_loop(move |ui| update_card_image(&ui, &state_for_ui, id));
    });
}

pub(super) fn cover_target_size() -> (u32, u32) {
    let (ui_scale, card_scale) = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            (
                settings.ui_scale_percent as f32 / 100.0,
                settings.card_scale_percent as f32 / 100.0,
            )
        })
        .unwrap_or((1.0, 1.0));

    // Keep a little resolution reserve for fractional DPI, but never retain the
    // original multi-megapixel cover: the software renderer otherwise rescales
    // it for every frame while the list is moving.
    let card_factor = ui_scale * card_scale;
    let width = ((320.0 * card_factor - 18.0 * card_factor) * 1.25)
        .round()
        .clamp(160.0, 720.0) as u32;
    let height = ((width as f32 * 9.0 / 16.0).round() as u32).clamp(90, 405);
    (width, height)
}

pub(super) fn decode_image_path(path: PathBuf, target_size: (u32, u32)) -> Option<ImagePixels> {
    let image = image::open(path)
        .ok()?
        .resize_to_fill(
            target_size.0,
            target_size.1,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    Some(ImagePixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

pub(super) fn resize_cover_pixels(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    target_size: (u32, u32),
) -> Option<ImagePixels> {
    let source = image::RgbaImage::from_raw(width, height, rgba)?;
    let resized = image::DynamicImage::ImageRgba8(source)
        .resize_to_fill(
            target_size.0,
            target_size.1,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    Some(ImagePixels {
        width: resized.width(),
        height: resized.height(),
        rgba: resized.into_raw(),
    })
}
pub(super) fn image_from_pixels(pixels: &ImagePixels) -> Image {
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &pixels.rgba,
        pixels.width,
        pixels.height,
    );
    Image::from_rgba8(buffer)
}

pub(super) fn prune_cover_cache(state: &SharedState) {
    let mut keep = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            settings
                .downloaded_games
                .iter()
                .map(|game| game.thread_id)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    if let Ok(state) = state.lock() {
        keep.extend(state.cards.iter().map(|card| card.id));
    }
    UI_IMAGE_CACHE.with(|cache| {
        cache.borrow_mut().covers.retain(|id, _| keep.contains(id));
    });
    if let Ok(mut state) = state.lock() {
        state.loaded_images.retain(|id| keep.contains(id));
    }
}

pub(super) fn finish_cover_decode(
    ui: &MainWindow,
    state: &SharedState,
    id: u64,
    decoded: Option<ImagePixels>,
) {
    if let Some(pixels) = decoded.filter(|_| should_keep_cover(state, id)) {
        let image = image_from_pixels(&pixels);
        UI_IMAGE_CACHE.with(|cache| {
            cache.borrow_mut().covers.insert(id, image);
        });
        if let Ok(mut state) = state.lock() {
            state.loaded_images.insert(id);
            state.loading_images.remove(&id);
        }
    } else if let Ok(mut state) = state.lock() {
        state.loading_images.remove(&id);
    }
    update_card_image(ui, state, id);
}

pub(super) fn should_keep_cover(state: &SharedState, id: u64) -> bool {
    let installed = crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| {
            settings
                .downloaded_games
                .iter()
                .any(|game| game.thread_id == id)
        })
        .unwrap_or(false);
    installed
        || state
            .lock()
            .map(|state| state.cards.iter().any(|card| card.id == id))
            .unwrap_or(false)
}

pub(super) fn cached_cover_image(id: u64) -> Image {
    UI_IMAGE_CACHE.with(|cache| cache.borrow().covers.get(&id).cloned().unwrap_or_default())
}

pub(super) fn cached_screen_image(id: u64, index: usize) -> Image {
    UI_IMAGE_CACHE.with(|cache| {
        cache
            .borrow()
            .screens
            .get(&(id, index))
            .cloned()
            .unwrap_or_default()
    })
}

pub(super) fn cache_screen_image(id: u64, index: usize, pixels: &ImagePixels) -> Vec<u64> {
    let limit = screen_cache_game_limit();
    UI_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.screens.insert((id, index), image_from_pixels(pixels));
        cache.screen_games.retain(|game_id| *game_id != id);
        cache.screen_games.push_back(id);
        trim_screen_cache(&mut cache, limit)
    })
}

fn screen_cache_game_limit() -> usize {
    crate::app::settings::APP_SETTINGS
        .read()
        .map(|settings| settings.image_cache_games.clamp(1, 100) as usize)
        .unwrap_or(12)
}

fn trim_screen_cache(cache: &mut UiImageCache, limit: usize) -> Vec<u64> {
    let mut evicted = Vec::new();
    while cache.screen_games.len() > limit.max(1) {
        if let Some(game_id) = cache.screen_games.pop_front() {
            cache
                .screens
                .retain(|(screen_id, _), _| *screen_id != game_id);
            evicted.push(game_id);
        }
    }
    evicted
}

pub(super) fn prune_screen_cache(ui: &MainWindow, state: &SharedState) {
    let evicted = UI_IMAGE_CACHE
        .with(|cache| trim_screen_cache(&mut cache.borrow_mut(), screen_cache_game_limit()));
    if evicted.is_empty() {
        return;
    }
    if let Ok(mut state) = state.lock() {
        for id in &evicted {
            state
                .loaded_screen_images
                .retain(|(screen_id, _)| screen_id != id);
            state.loaded_screens.remove(id);
        }
    }
    for id in evicted {
        update_card_image(ui, state, id);
    }
}
