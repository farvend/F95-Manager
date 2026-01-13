use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::{CardImageProvider, ImageData, LibraryCard};

pub enum ImageMsg {
    CoverOk {
        thread_id: u64,
        data: ImageData,
    },
    CoverErr {
        thread_id: u64,
    },
    ScreenOk {
        thread_id: u64,
        idx: usize,
        data: ImageData,
    },
    ScreenErr {
        thread_id: u64,
        idx: usize,
    },
}

pub struct LibraryCardManager {
    provider: Arc<dyn CardImageProvider>,
    cards: Vec<LibraryCard>,
    covers: HashMap<u64, egui::TextureHandle>,
    covers_loading: HashSet<u64>,
    screens: HashMap<u64, Vec<Option<egui::TextureHandle>>>,
    screens_loading: HashSet<(u64, usize)>,
    tx: std::sync::mpsc::Sender<ImageMsg>,
    rx: std::sync::mpsc::Receiver<ImageMsg>,
}

impl LibraryCardManager {
    pub fn new(provider: Arc<dyn CardImageProvider>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            provider,
            cards: Vec::new(),
            covers: HashMap::new(),
            covers_loading: HashSet::new(),
            screens: HashMap::new(),
            screens_loading: HashSet::new(),
            tx,
            rx,
        }
    }

    pub fn set_cards(&mut self, cards: Vec<LibraryCard>) {
        self.cards = cards;
    }

    pub fn cards(&self) -> &[LibraryCard] {
        &self.cards
    }

    pub fn get_cover(&self, thread_id: u64) -> Option<&egui::TextureHandle> {
        self.covers.get(&thread_id)
    }

    pub fn get_screen(&self, thread_id: u64, idx: usize) -> Option<&egui::TextureHandle> {
        self.screens
            .get(&thread_id)
            .and_then(|v| v.get(idx))
            .and_then(|opt| opt.as_ref())
    }

    pub fn get_screens_slice(&self, thread_id: u64) -> Option<&[Option<egui::TextureHandle>]> {
        self.screens.get(&thread_id).map(|v| v.as_slice())
    }

    pub fn is_cover_loading(&self, thread_id: u64) -> bool {
        self.covers_loading.contains(&thread_id)
    }

    pub fn schedule_cover_download(&mut self, card: &LibraryCard) {
        let id = card.thread_id;
        if self.covers.contains_key(&id) || self.covers_loading.contains(&id) {
            return;
        }
        if card.cover_url.is_none() {
            return;
        }

        self.covers_loading.insert(id);
        let provider = self.provider.clone();
        let card_clone = card.clone();
        let tx = self.tx.clone();

        crate::app::rt().spawn(async move {
            match provider.fetch_cover(&card_clone).await {
                Ok(data) => {
                    let _ = tx.send(ImageMsg::CoverOk {
                        thread_id: id,
                        data,
                    });
                }
                Err(e) => {
                    log::warn!("cover fetch failed: id={} err={}", id, e);
                    let _ = tx.send(ImageMsg::CoverErr { thread_id: id });
                }
            }
        });
    }

    pub fn schedule_screen_download(&mut self, card: &LibraryCard, idx: usize) {
        let id = card.thread_id;
        if idx >= card.screen_urls.len() {
            return;
        }
        if self.screens_loading.contains(&(id, idx)) {
            return;
        }

        let entry = self
            .screens
            .entry(id)
            .or_insert_with(|| vec![None; card.screen_urls.len()]);
        if entry.len() < card.screen_urls.len() {
            entry.resize_with(card.screen_urls.len(), || None);
        }
        if entry.get(idx).and_then(|s| s.as_ref()).is_some() {
            return;
        }

        self.screens_loading.insert((id, idx));
        let provider = self.provider.clone();
        let card_clone = card.clone();
        let tx = self.tx.clone();

        crate::app::rt().spawn(async move {
            match provider.fetch_screen(&card_clone, idx).await {
                Ok(data) => {
                    let _ = tx.send(ImageMsg::ScreenOk {
                        thread_id: id,
                        idx,
                        data,
                    });
                }
                Err(e) => {
                    log::warn!("screen fetch failed: id={} idx={} err={}", id, idx, e);
                    let _ = tx.send(ImageMsg::ScreenErr { thread_id: id, idx });
                }
            }
        });
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                ImageMsg::CoverOk { thread_id, data } => {
                    self.covers_loading.remove(&thread_id);
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [data.width as usize, data.height as usize],
                        &data.rgba,
                    );
                    let tex = ctx.load_texture(
                        format!("cover_{}", thread_id),
                        image,
                        egui::TextureOptions::default(),
                    );
                    self.covers.insert(thread_id, tex);
                }
                ImageMsg::CoverErr { thread_id } => {
                    self.covers_loading.remove(&thread_id);
                }
                ImageMsg::ScreenOk {
                    thread_id,
                    idx,
                    data,
                } => {
                    self.screens_loading.remove(&(thread_id, idx));
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [data.width as usize, data.height as usize],
                        &data.rgba,
                    );
                    let tex = ctx.load_texture(
                        format!("screen_{}_{}", thread_id, idx),
                        image,
                        egui::TextureOptions::default(),
                    );
                    let entry = self.screens.entry(thread_id).or_insert_with(Vec::new);
                    if entry.len() <= idx {
                        entry.resize_with(idx + 1, || None);
                    }
                    entry[idx] = Some(tex);
                }
                ImageMsg::ScreenErr { thread_id, idx } => {
                    self.screens_loading.remove(&(thread_id, idx));
                }
            }
            ctx.request_repaint();
        }
    }
}
