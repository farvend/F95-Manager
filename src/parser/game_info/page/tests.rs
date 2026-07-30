#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::parser::game_info::{
        Platform,
        page::{F95Page, GetLinksError},
    };

    #[test]
    fn hidden_guest_links_return_login_required() {
        let page = F95Page(
            r#"
                <html data-logged-in="false">
                    <body>
                        <div style="text-align: center">
                            <b><span style="font-size: 22px">DOWNLOAD</span></b><br />
                            <span style="font-size: 18px"><b>Win</b>:
                                <div class="messageHide messageHide--link">
                                    You must be registered to see the links
                                </div>
                            </span>
                        </div>
                    </body>
                </html>
            "#
            .to_string(),
        );

        let err = page.get_download_links().unwrap_err();
        assert!(matches!(err, GetLinksError::LoginRequired));
    }

    #[test]
    fn unsupported_download_hostings_return_specific_error() {
        let page = F95Page(
            r#"
                <html>
                    <body>
                        <div style="text-align: center">
                            <b><span style="font-size: 22px">DOWNLOAD</span></b><br />
                            <span style="font-size: 18px"><b>Win</b>:
                                <a href="https://bunkr.site/f/example">BUNKR</a> -
                                <a href="https://datanodes.to/example.zip">DATANODES</a>
                            </span><br />
                        </div>
                    </body>
                </html>
            "#
            .to_string(),
        );

        let err = page.get_download_links().unwrap_err();
        assert!(matches!(err, GetLinksError::NoSupportedHostings));
    }

    #[test]
    fn platform_names_allow_whitespace_and_html_entities() {
        let page = F95Page(
            r#"
                <div style="text-align: center">
                    <b>DOWNLOAD</b><br>
                    <span>Build 1.04</span><span><b>  Win &amp; Mac</b>: <a href="https://f95zone.to/masked/gofile.io/example">GOFILE</a></span><br>
                    <b>Android </b>: <a href="https://f95zone.to/masked/mega.nz/example">MEGA</a><br>
                    <b>Linux </b>: <a href="https://f95zone.to/masked/pixeldrain.com/example">PIXELDRAIN</a><br>
                    <b>Mac </b>: <a href="https://f95zone.to/masked/gofile.io/example">GOFILE</a><br>
                </div>
            "#
            .to_string(),
        );

        let downloads = page.get_download_links().unwrap();
        let platforms: Vec<Platform> = downloads.iter().map(|item| *item.platform()).collect();

        assert_eq!(
            platforms,
            vec![
                Platform::WINDOWS | Platform::MAC,
                Platform::ANDROID,
                Platform::LINUX,
                Platform::MAC,
            ]
        );
    }

    #[test]
    fn download_heading_is_case_insensitive() {
        let page = F95Page(
            r#"
                <div style="text-align: center">
                    <span><b>Download<br>
                    Win: </b><a href="https://files.catbox.moe/example.zip">CATBOX</a><br>
                </div>
            "#
            .to_string(),
        );

        let downloads = page.get_download_links().unwrap();

        assert_eq!(downloads.len(), 1);
        assert_eq!(*downloads[0].platform(), Platform::WINDOWS);
    }

    #[test]
    #[ignore = "Requires big HTML files with potentially sensitive data"]
    fn test_all_pages() {
        let pages_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/parser/game_info/page/pages");

        let entries = fs::read_dir(&pages_dir)
            .unwrap_or_else(|e| panic!("Failed to read pages dir {:?}: {}", pages_dir, e));

        let mut count = 0;
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "html") {
                let content = fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", path, e));
                let page = F95Page(content);
                page.get_download_links()
                    .unwrap_or_else(|e| panic!("Failed to parse {:?}: {}", path, e));
                count += 1;
            }
        }

        assert!(count > 0, "No HTML files found in {:?}", pages_dir);
    }
}
