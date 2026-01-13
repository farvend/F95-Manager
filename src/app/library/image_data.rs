/// Raw RGBA image data for transfer between async tasks and UI.
#[derive(Debug, Clone)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ImageData {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        debug_assert_eq!(
            rgba.len(),
            (width * height * 4) as usize,
            "RGBA buffer size mismatch"
        );
        Self {
            width,
            height,
            rgba,
        }
    }
}
