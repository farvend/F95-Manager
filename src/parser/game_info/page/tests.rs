#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::parser::game_info::page::{F95Page, GetLinksError};

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
