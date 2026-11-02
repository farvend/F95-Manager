use std::fmt;

use reqwest::Url;
use strum::EnumString;

// Macro to define an enum with a "subset" companion enum and conversions
macro_rules! define_subset {
    (
        $(#[$meta:meta])*
        pub enum $name:ident, $subset_name:ident {
            subset: { $($subset:ident),* $(,)? }
            general: { $($general:ident),* $(,)? }
        }
    ) => {
        $(#[$meta])*
        pub enum $name {
            $($subset,)*
            $($general,)*
        }

        $(#[$meta])*
        pub enum $subset_name {
            $($subset,)*
        }

        impl $name {
            #[allow(unused)]
            pub fn as_subset(&self) -> Option<$subset_name> {
                match self {
                    $( Self::$subset => Some($subset_name::$subset), )*
                    _ => None,
                }
            }
        }

        impl From<$subset_name> for $name {
            fn from(subset: $subset_name) -> Self {
                match subset {
                    $( $subset_name::$subset => $name::$subset, )*
                }
            }
        }
        impl TryFrom<$name> for $subset_name {
            type Error = ();
            fn try_from(value: $name) -> Result<Self, Self::Error> {
                match value {
                    $( $name::$subset => Ok($subset_name::$subset), )*
                    _ => Err(()),
                }
            }
        }
    };
}

define_subset! {
    #[derive(EnumString, Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Hosting, HostingSubset {
        subset: {
            Pixeldrain,
            Gofile,
            Mega,
            Catbox
        }
        general: {
            Mediafire,
            Workupload,
            Uploadhaven,
            Racaty,
            Zippy,
            Nopy,
            Mixdrop,
        }
    }
}

impl fmt::Display for Hosting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let url = match self {
            Hosting::Gofile => "gofile.io",
            Hosting::Mediafire => "mediafire.com",
            Hosting::Mega => "mega.nz",
            Hosting::Mixdrop => "mixdrop.sn",
            Hosting::Nopy => "nopy.to",
            Hosting::Pixeldrain => "pixeldrain.com",
            Hosting::Racaty => "racaty.com",
            Hosting::Uploadhaven => "uploadhaven.com",
            Hosting::Workupload => "workupload.com",
            Hosting::Zippy => "zippyshare.com",
            Hosting::Catbox => "files.catbox.moe",
        };
        write!(f, "{url}")
    }
}

impl Hosting {
    pub fn base(&self) -> &'static str {
        "https://"
    }
}

#[derive(Debug)]
pub enum HostingError {
    NotDomain,
    UnknownDomain,
}

impl TryFrom<Url> for Hosting {
    type Error = HostingError;
    fn try_from(value: Url) -> Result<Self, HostingError> {
        let sec = value
            .domain()
            .ok_or(HostingError::NotDomain)?
            .split('.')
            .rev()
            .nth(1)
            .ok_or(HostingError::NotDomain)?
            .to_string();

        let mut name = sec;
        if let Some(s) = name.get_mut(0..1) {
            s.make_ascii_uppercase()
        }
        name.parse().map_err(|_| HostingError::UnknownDomain)
    }
}

#[derive(Debug)]
pub enum HostingSubsetError {
    HostingError(HostingError),
    UnsopportedHosting,
}

impl TryFrom<Url> for HostingSubset {
    type Error = HostingSubsetError;
    fn try_from(value: Url) -> Result<Self, Self::Error> {
        let hosting: Hosting = value.try_into().map_err(HostingSubsetError::HostingError)?;
        hosting
            .try_into()
            .map_err(|_| HostingSubsetError::UnsopportedHosting)
    }

    // fn from(value: Url) -> Self {
    //     let hosting: Hosting = value.into();
    //     hosting.as_subset().unwrap()
    // }
}

impl HostingSubset {
    pub fn base(&self) -> &'static str {
        let hosting: Hosting = (*self).into();
        hosting.base()
    }

    /// Supported hosting named by the text F95 puts on a `/dl/` link
    /// ("GOFILE", "MEGA", ...). `None` for hostings the app cannot download.
    pub fn from_label(label: &str) -> Option<HostingSubset> {
        match label.trim().to_ascii_lowercase().as_str() {
            "pixeldrain" => Some(HostingSubset::Pixeldrain),
            "gofile" => Some(HostingSubset::Gofile),
            "mega" => Some(HostingSubset::Mega),
            "catbox" => Some(HostingSubset::Catbox),
            _ => None,
        }
    }
}

impl fmt::Display for HostingSubset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hosting: Hosting = (*self).into();
        hosting.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn single_label_host_is_rejected_without_panicking() {
        let url = Url::from_str("https://localhost/file.zip").expect("url");

        assert!(matches!(
            Hosting::try_from(url),
            Err(HostingError::NotDomain)
        ));
    }
}
