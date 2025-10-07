// UI constants extracted from scattered magic numbers across the codebase.
// Clean Code principle: Replace Magic Numbers with Named Constants.

/// Maximum number of filter items (tags/prefixes) that can be selected
pub const MAX_FILTER_ITEMS: usize = 10;

/// Maximum number of filter items as string (for UI display)
pub const MAX_FILTER_ITEMS_STR: &str = "10";

/// Default card width in logical pixels
pub const CARD_WIDTH: f32 = 320.0;

/// Gap between cards in the grid
pub const CARD_GAP: f32 = 16.0;

/// Debounce delay for search query in milliseconds
pub const SEARCH_DEBOUNCE_MS: u64 = 300;

/// UI spacing constants
pub mod spacing {
    /// Small spacing (4px)
    pub const SMALL: f32 = 4.0;
    
    /// Medium spacing (8px)
    pub const MEDIUM: f32 = 8.0;
    
    /// Large spacing (16px)
    pub const LARGE: f32 = 16.0;
    
    /// Extra large spacing (24px)
    pub const XLARGE: f32 = 24.0;
}

/// Card-specific layout constants
pub mod card {
    /// Inner margin of card frame (symmetric)
    pub const INNER_MARGIN: f32 = 8.0;
    
    /// Border radius of card corners
    pub const ROUNDING: f32 = 8.0;
    
    /// Space after cover image
    pub const POST_COVER_GAP: f32 = 20.0;
    
    /// Stats plaque rounding
    pub const STATS_ROUNDING: f32 = 6.0;
    
    /// Stats plaque inner margin (horizontal)
    pub const STATS_MARGIN_H: f32 = 8.0;
    
    /// Stats plaque inner margin (vertical)
    pub const STATS_MARGIN_V: f32 = 6.0;
}

/// Download progress weights
pub mod download {
    /// Weight of download phase in overall progress
    pub const DOWNLOAD_WEIGHT: f32 = 0.75;
    
    /// Weight of unzip phase in overall progress
    pub const UNZIP_WEIGHT: f32 = 0.25;
}
