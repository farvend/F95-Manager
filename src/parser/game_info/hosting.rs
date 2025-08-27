use std::fmt;

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

impl HostingSubset {
    pub fn base(&self) -> &'static str {
        let hosting: Hosting = (*self).into();
        hosting.base()
    }
}

impl fmt::Display for HostingSubset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hosting: Hosting = (*self).into();
        hosting.fmt(f)
    }
}
