#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use reqwest::Url;

    use crate::parser::game_info::DownloadLink;
    use crate::parser::game_info::link::{
        parse_content_disposition_filename, sanitize_filename,
    };

    #[test]
    fn masked_links_are_accepted_only_for_supported_hosting() {
        // Supported hosting in subset
        let ok = Url::from_str("https://f95zone.to/masked/pixeldrain.com/whatever").unwrap();
        assert!(matches!(DownloadLink::new(ok), Some(DownloadLink::Masked(_))));

        // Unsupported hosting (not in HostingSubset)
        let bad = Url::from_str("https://f95zone.to/masked/mediafire.com/whatever").unwrap();
        assert!(DownloadLink::new(bad).is_none());
    }

    #[test]
    fn direct_links_are_supported_for_known_subset_hosts() {
        let url = Url::from_str("https://pixeldrain.com/u/abcdef").unwrap();
        assert!(matches!(DownloadLink::new(url), Some(DownloadLink::Direct(_))));
    }

    #[test]
    fn content_disposition_filename_is_sanitized() {
        // Matches common header forms; ensure invalid chars are removed.
        let raw = "attachment; filename=\"a<b>c:d*e?f|g\\h/.. .zip\"";
        let parsed = parse_content_disposition_filename(raw).unwrap();
        let sanitized = sanitize_filename(&parsed);

        assert!(!sanitized.contains('<'));
        assert!(!sanitized.contains('>'));
        assert!(!sanitized.contains(':'));
        assert!(!sanitized.contains('*'));
        assert!(!sanitized.contains('?'));
        assert!(!sanitized.contains('|'));
        assert!(!sanitized.contains('\\'));
        assert!(!sanitized.contains('/'));
        assert!(!sanitized.ends_with(' '));
        assert!(!sanitized.ends_with('.'));
    }

    #[test]
    fn content_disposition_filename_reserved_windows_device_names_are_avoided() {
        let raw = "attachment; filename=\"con.txt\"";
        let parsed = parse_content_disposition_filename(raw).unwrap();
        let sanitized = sanitize_filename(&parsed);
        assert_ne!(sanitized.to_ascii_uppercase(), "CON.TXT");
        assert!(sanitized.starts_with('_'));
    }

    #[test]
    fn content_disposition_filename_rfc2047_encoded_word_is_decoded() {
        // "hello.txt" in base64
        let raw = "attachment; filename=\"=?UTF-8?B?aGVsbG8udHh0?=\"";
        assert_eq!(
            parse_content_disposition_filename(raw),
            Some("hello.txt".to_string())
        );
    }

    #[test]
    fn content_disposition_filename_star_is_parsed() {
        let raw = "attachment; filename*=UTF-8''hello.zip";
        assert_eq!(parse_content_disposition_filename(raw), Some("hello.zip".to_string()));
    }

    #[test]
    fn content_disposition_filename_star_is_percent_decoded() {
        let raw = "attachment; filename*=UTF-8''hello%20world.zip";
        assert_eq!(
            parse_content_disposition_filename(raw),
            Some("hello world.zip".to_string())
        );
    }
}
