use crate::views::filters::EnumWithAlternativeNames;

#[derive(strum::EnumCount, strum::EnumIter, PartialEq, Clone, strum::Display, Default, Debug)]
pub enum Sorting {
    #[default]
    Date,
    Likes,
    Views,
    Title,
    Rating,
}
impl EnumWithAlternativeNames for Sorting {
    fn alternative_name(&self) -> &'static str {
        use Sorting::*;
        match self {
            Date => "🕓",
            Likes => "👍",
            Views => "👀",
            Title => "🔤",
            Rating => "⭐",
        }
    }
}

impl Sorting {
    // Mapping to F95 API expected values
    pub fn api_value(&self) -> &'static str {
        match self {
            Sorting::Date => "date",
            Sorting::Likes => "likes",
            Sorting::Views => "views",
            Sorting::Title => "alpha",
            Sorting::Rating => "rating",
        }
    }
}

#[derive(strum::EnumCount, strum::EnumIter, PartialEq, Clone, Copy, Default)]
pub enum DateLimit {
    #[default]
    Anytime,
    Today,
    Days3,
    Days7,
    Days14,
    Days30,
    Days90,
    Days180,
    Days365,
}
impl std::fmt::Display for DateLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use DateLimit::*;
        let s = match self {
            Anytime => "ANYTIME",
            Today => "TODAY",
            Days3 => "LAST 3 DAYS",
            Days7 => "LAST 7 DAYS",
            Days14 => "LAST 14 DAYS",
            Days30 => "LAST 30 DAYS",
            Days90 => "LAST 90 DAYS",
            Days180 => "LAST 180 DAYS",
            Days365 => "LAST 365 DAYS",
        };
        f.write_str(s)
    }
}

#[derive(strum::EnumCount, strum::EnumIter, PartialEq, Clone, Default)]
pub enum TagLogic {
    #[default]
    Or,
    And,
}
impl EnumWithAlternativeNames for TagLogic {
    fn alternative_name(&self) -> &'static str {
        match self {
            TagLogic::Or => "OR",
            TagLogic::And => "AND",
        }
    }
}

// Search mode for header switcher
#[derive(strum::EnumCount, strum::EnumIter, PartialEq, Clone, Default)]
pub enum SearchMode {
    Creator,
    #[default]
    Title,
}

impl EnumWithAlternativeNames for SearchMode {
    fn alternative_name(&self) -> &'static str {
        match self {
            SearchMode::Creator => "CREATOR",
            SearchMode::Title => "TITLE",
        }
    }
}

// Localization keys for enums used in Filters UI
impl crate::views::filters::LocalizableName for Sorting {
    fn loc_key(&self) -> &'static str {
        match self {
            Sorting::Date => "sorting-date",
            Sorting::Likes => "sorting-likes",
            Sorting::Views => "sorting-views",
            Sorting::Title => "sorting-title",
            Sorting::Rating => "sorting-rating",
        }
    }
}

impl crate::views::filters::LocalizableName for DateLimit {
    fn loc_key(&self) -> &'static str {
        match self {
            DateLimit::Anytime => "date-limit-anytime",
            DateLimit::Today => "date-limit-today",
            DateLimit::Days3 => "date-limit-days3",
            DateLimit::Days7 => "date-limit-days7",
            DateLimit::Days14 => "date-limit-days14",
            DateLimit::Days30 => "date-limit-days30",
            DateLimit::Days90 => "date-limit-days90",
            DateLimit::Days180 => "date-limit-days180",
            DateLimit::Days365 => "date-limit-days365",
        }
    }
}

impl crate::views::filters::LocalizableName for TagLogic {
    fn loc_key(&self) -> &'static str {
        match self {
            TagLogic::Or => "tag-logic-or",
            TagLogic::And => "tag-logic-and",
        }
    }
}

impl crate::views::filters::LocalizableName for SearchMode {
    fn loc_key(&self) -> &'static str {
        match self {
            SearchMode::Creator => "search-mode-creator",
            SearchMode::Title => "search-mode-title",
        }
    }
}
