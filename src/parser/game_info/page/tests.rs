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
fn ignores_lightbox_phrase_before_download_block() {
    // XenForo puts `"lightbox_download": "Download"` in a `js-extraPhrases`
    // JSON block above the real DOWNLOAD heading; a case-insensitive match
    // picks that block and loses every link.
    let page = F95Page(
        r#"
                <div style="text-align: center">
                    <script class="js-extraPhrases" type="application/json">
                        { "lightbox_download": "Download", "lightbox_close": "Close" }
                    </script>
                </div>
                <div style="text-align: center">
                    <b><span style="font-size: 22px">DOWNLOAD</span></b><br />
                    <span style="font-size: 18px"><b>Win</b>:
                        <a href="https://f95zone.to/dl/21783444/?u=1&amp;k=2&amp;r=3&amp;t=4">MEGA</a>
                    </span><br />
                </div>
            "#
        .to_string(),
    );

    let downloads = page.get_download_links().unwrap();

    assert_eq!(downloads.len(), 1);
    assert_eq!(*downloads[0].platform(), Platform::WINDOWS);
    assert_eq!(downloads[0].links().len(), 1);
    assert!(matches!(
        downloads[0].links()[0],
        crate::parser::game_info::DownloadLink::F95Page(_)
    ));
}

#[test]
fn skips_hidden_decoy_links_and_keeps_query_and_host() {
    // Real markup from thread 320895: a hidden copy of the first link sits in
    // front of it, and every `/dl/` link only works with its full query string.
    let page = F95Page(
        r#"
                <div style="text-align: center">
                    <b><span style="font-size: 22px">DOWNLOAD</span></b><br />
                    <span style="font-size: 18px"><b>Win</b>:
                        <span class="u-hidden js-noSelectToQuote" aria-hidden="true"><a href="https://f95zone.to/dl/22078057/?u=1&amp;k=decoy&amp;r=3&amp;t=4" tabindex="-1">GOFILE</a></span><a href="https://f95zone.to/dl/22078057/?u=1&amp;k=gofile&amp;r=3&amp;t=4">GOFILE</a> - <a href="https://f95zone.to/dl/22078057/?u=1&amp;k=mega&amp;r=3&amp;t=4">MEGA</a>
                    </span><br />
                </div>
            "#
        .to_string(),
    );

    let downloads = page.get_download_links().unwrap();
    let links: Vec<(String, Option<String>)> = downloads[0]
        .links()
        .iter()
        .map(|link| match link {
            crate::parser::game_info::DownloadLink::F95Page(page) => {
                (page.host.clone(), page.url.query().map(str::to_string))
            }
            other => panic!("expected F95Page, got {other:?}"),
        })
        .collect();

    assert_eq!(
        links,
        vec![
            ("GOFILE".to_string(), Some("u=1&k=gofile&r=3&t=4".to_string())),
            ("MEGA".to_string(), Some("u=1&k=mega&r=3&t=4".to_string())),
        ]
    );
}

#[test]
fn unknown_platform_keeps_links_for_manual_selection() {
    let page = F95Page(
        r#"
                <div style="text-align: center">
                    <b>DOWNLOAD</b><br>
                    <b>Browser build:</b>
                    <a href="https://files.catbox.moe/example.zip">CATBOX</a><br>
                </div>
            "#
        .to_string(),
    );

    let downloads = page.get_download_links().unwrap();

    assert_eq!(downloads.len(), 1);
    assert_eq!(*downloads[0].platform(), Platform::OTHER);
    assert_eq!(downloads[0].links().len(), 1);
}

#[test]
#[ignore = "Requires big HTML files with potentially sensitive data"]
fn test_all_pages() {
    let pages_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/parser/game_info/page/pages");

    let entries = fs::read_dir(&pages_dir)
        .unwrap_or_else(|e| panic!("Failed to read pages dir {:?}: {}", pages_dir, e));

    let mut count = 0;
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "html") {
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
