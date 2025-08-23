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

// Additional filter enums and constants

#[derive(strum::EnumCount, strum::EnumIter, PartialEq, Clone, Copy, Default)]
pub enum DateLimit {
    #[default]
    Anytime,
    Days3,
    Week1,
    Month1,
    Month3,
    Year1,
}
impl std::fmt::Display for DateLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use DateLimit::*;
        let s = match self {
            Anytime => "ANYTIME",
            Days3 => "LAST 3 DAYS",
            Week1 => "LAST 7 DAYS",
            Month1 => "LAST MONTH",
            Month3 => "LAST 3 MONTHS",
            Year1 => "LAST YEAR",
        };
        f.write_str(s)
    }
}
// НЕ ИСПОЛЬЗУЙ ЭТО. ИСПОЛЬЗУЙ ТОЛЬКО pub enum И МОЖЕТ ЧТО-ТО ИЗ STRUM ОТ НЕГО
// const DATE_LIMIT_VALUES: [DateLimit; 6] = [
//     DateLimit::Anytime,
//     DateLimit::Days3,
//     DateLimit::Week1,
//     DateLimit::Month1,
//     DateLimit::Month3,
//     DateLimit::Year1,
// ];

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

// #[derive(strum::EnumCount, strum::EnumIter, PartialEq, Clone, Default)]
// pub enum TagKind {
//     #[default]
//     VisualNovel,
//     RPG,
//     Sandbox,
//     SciFi,
//     Fantasy,
//     Horror,
//     Mystery,
//     Comedy,
//     School,
//     Romance,
// }
// impl EnumWithAlternativeNames for TagKind {
//     fn alternative_name(&self) -> &'static str {
//         match self {
//             TagKind::VisualNovel => "Visual Novel",
//             TagKind::RPG => "RPG",
//             TagKind::Sandbox => "Sandbox",
//             TagKind::SciFi => "Sci-Fi",
//             TagKind::Fantasy => "Fantasy",
//             TagKind::Horror => "Horror",
//             TagKind::Mystery => "Mystery",
//             TagKind::Comedy => "Comedy",
//             TagKind::School => "School",
//             TagKind::Romance => "Romance",
//         }
//     }
// }

// impl std::fmt::Display for TagKind {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.write_str(self.alternative_name())
//     }
// }

// #[derive(strum::EnumCount, strum::EnumIter, PartialEq, Clone, Default)]
// pub enum NoMode {
//     #[default]
//     None,
// }
// impl EnumWithAlternativeNames for NoMode {
//     fn alternative_name(&self) -> &'static str {
//         ""
//     }
// }

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

// fn map_sort(s: &Sorting) -> crate::parser::SortParam {
//     match s {
//         Sorting::Date => crate::parser::SortParam::Date,
//         Sorting::Likes => crate::parser::SortParam::Likes,
//         Sorting::Views => crate::parser::SortParam::Views,
//         Sorting::Alphabetical => crate::parser::SortParam::Alphabetical,
//         Sorting::Score => crate::parser::SortParam::Rating,
//     }
// }
