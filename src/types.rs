#[derive(strum::EnumCount, strum::EnumIter, PartialEq, Clone, strum::Display, Default, Debug)]
pub enum Sorting {
    #[default]
    Date,
    Likes,
    Views,
    Title,
    Rating,
}

impl Sorting {
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
        let value = match self {
            DateLimit::Anytime => "ANYTIME",
            DateLimit::Today => "TODAY",
            DateLimit::Days3 => "LAST 3 DAYS",
            DateLimit::Days7 => "LAST 7 DAYS",
            DateLimit::Days14 => "LAST 14 DAYS",
            DateLimit::Days30 => "LAST 30 DAYS",
            DateLimit::Days90 => "LAST 90 DAYS",
            DateLimit::Days180 => "LAST 180 DAYS",
            DateLimit::Days365 => "LAST 365 DAYS",
        };
        f.write_str(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TagLogic {
    #[default]
    Or,
    And,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    Creator,
    #[default]
    Title,
}
