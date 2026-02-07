"""
Script for restoring the F95 Manager library by scanning a games directory and
matching folders with cached thread metadata.

References:
- DownloadedGame: thread_id, folder, exe_path, has_been_launched, bookmark_ids
  (from src/app/settings/store.rs)
- CachedThreadMeta: thread_id, title, creator, version, cover_url, screens, tag_ids
  (from src/app/library/metadata_codec.rs)
"""

import argparse
import json
import logging
import re
import sys
from pathlib import Path
from typing import Dict, Optional


def setup_logging():
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
        handlers=[logging.StreamHandler(sys.stdout)],
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Restore F95 Manager library from existing game folders."
    )
    parser.add_argument(
        "--cache-dir",
        type=str,
        default="cache",
        help='Path to the cache directory (default: "cache")',
    )
    parser.add_argument(
        "--games-dir",
        type=str,
        required=True,
        help="Path to the directory containing installed games",
    )
    parser.add_argument(
        "--settings-file",
        type=str,
        default="app_settings.json",
        help='Path to the application settings file (default: "app_settings.json")',
    )
    parser.add_argument(
        "--threshold",
        type=int,
        default=90,
        help="Fuzzy match threshold percentage (default: 90)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview changes without saving to settings file",
    )
    return parser.parse_args()


def normalize_title(title: str) -> str:
    """
    Normalizes a title by:
    - Converting to lowercase
    - Removing version patterns (v1.0, 0.5.6, etc)
    - Removing special characters, keeping only alphanumeric
    """
    # 1. Lowercase
    t = title.lower()
    # 2. Remove version patterns
    # Matches patterns like v1.2.3, 0.5.6, [v1.0], (v1.0)
    # We look for digits separated by dots, optionally prefixed by 'v'
    t = re.sub(r"[\[\(\s]v?\d+(\.\d+)+[\]\)\s]?", " ", t)
    # Also handle cases where version is at the very beginning or end
    t = re.sub(r"^v?\d+(\.\d+)+", " ", t)
    t = re.sub(r"v?\d+(\.\d+)+$", " ", t)
    # 3. Keep only alphanumeric
    t = re.sub(r"[^a-z0-9]", "", t)
    return t


def load_cache(cache_dir: Path) -> Dict[int, dict]:
    """
    Scans cache_dir, reads meta.json, validates required fields,
    and returns a dictionary mapping thread_id to the metadata.
    """
    logger = logging.getLogger(__name__)
    cache = {}
    success_count = 0
    fail_count = 0

    if not cache_dir.exists():
        logger.error(f"Cache directory not found: {cache_dir}")
        return cache

    for entry in cache_dir.iterdir():
        if not entry.is_dir():
            continue

        meta_file = entry / "meta.json"
        if not meta_file.exists():
            continue

        try:
            with open(meta_file, "r", encoding="utf-8") as f:
                data = json.load(f)

            # Validation: required fields from metadata_codec.rs
            if "thread_id" not in data or "title" not in data:
                logger.warning(f"Skipping {meta_file}: missing required fields (thread_id/title)")
                fail_count += 1
                continue

            thread_id = data["thread_id"]
            if not isinstance(thread_id, int):
                try:
                    thread_id = int(thread_id)
                except (ValueError, TypeError):
                    logger.warning(f"Skipping {meta_file}: thread_id is not an integer")
                    fail_count += 1
                    continue

            cache[thread_id] = data
            success_count += 1

        except Exception as e:
            logger.warning(f"Error reading {meta_file}: {e}")
            fail_count += 1

    logger.info(f"Cache loaded: {success_count} entries ({fail_count} failed)")
    return cache


def main():
    setup_logging()
    args = parse_args()
    logger = logging.getLogger(__name__)

    logger.info("Starting library restoration...")
    logger.info(f"Games directory: {args.games_dir}")
    logger.info(f"Cache directory: {args.cache_dir}")
    logger.info(f"Settings file: {args.settings_file}")
    logger.info(f"Threshold: {args.threshold}%")
    if args.dry_run:
        logger.info("Dry run enabled - no changes will be saved.")

    cache = load_cache(Path(args.cache_dir))
    if not cache:
        logger.error("No cache entries loaded. Check your cache directory.")
        return

    # TODO: Implement scanning and matching logic in subsequent tasks
    logger.info("Initial setup complete. Ready for implementation of matching logic.")


if __name__ == "__main__":
    main()
