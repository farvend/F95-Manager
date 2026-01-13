#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::parser::game_info::page::F95Page;

    #[test]
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
