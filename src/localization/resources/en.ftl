app-window-title = F95 standalone client

settings-window-title = Settings
settings-temp-folder = Temp folder:
settings-extract-folder = Extract-to folder:
settings-cache-folder = Cache folder:
settings-language = Language:
settings-language-auto = Auto (System)
settings-language-en = English
settings-language-ru = Русский
settings-loading-anim = Loading indicator:
settings-loading-anim-bottom-bar = Bottom bar
settings-loading-anim-circle-bottom-right = Circle (bottom-right)
settings-custom-launch = Custom launch command (use {{path}} placeholder):
settings-cache-on-download = Cache metadata and images on download
settings-save = Save
settings-cancel = Cancel
settings-log-to-file = Write logs to file (warn and error)
settings-show-unplayed-badge = Show unplayed games badge
settings-classic-library-toggle = Use classic “Downloaded” button

settings-bookmarks-header = Bookmarks
settings-bookmarks-mgmt-btn = Manage Bookmarks...
settings-bookmarks-visible-limit = Bookmarks visible on cover:
settings-bookmarks-default-color = Default bookmark color:


auth-login-title = Login
auth-username = Username:
auth-password = Password:
auth-login-button = Login
auth-authorizing = Authorizing...
auth-or-paste-cookies = Or paste cookies (Cookie header):
auth-use-cookies = Use cookies
auth-please-enter-credentials = Please enter username and password
auth-please-paste-cookies = Please paste cookies
auth-info-needed = This information is needed to get download links from games' pages

loading = Loading...
error-prefix = Error: { $err }
pagination-page = Page { $cur } / { $total }
library-summary = Library: { $shown } / { $installed } found

# Filters panel
filters-title = Filters
filters-sorting = SORTING
filters-date-limit = DATE LIMIT
filters-search = SEARCH
filters-include-tags-header = TAGS (MAX { $max })
filters-exclude-tags-header = EXCLUDE TAGS (MAX { $max })
filters-include-prefixes-header = PREFIXES (MAX { $max })
filters-exclude-prefixes-header = EXCLUDE PREFIXES (MAX { $max })
filters-select-tag-include = Select a tag to filter...
filters-select-tag-exclude = Select a tag to exclude...
filters-select-prefix-include = Select a prefix to filter...
filters-select-prefix-exclude = Select a prefix to exclude...

filters-bookmarks-header = MY BOOKMARKS
filters-select-bookmark = Select bookmark...
filters-unplayed = Unplayed
filters-unplayed-on = Unplayed (ON)
filters-library-button = Downloaded
filters-library-button-on = Downloaded (ON)

filters-search-placeholder = Search...

# Enum localized names
sorting-date = Date
sorting-likes = Likes
sorting-views = Views
sorting-title = Title
sorting-rating = Rating

view-mode-header = MODE
view-mode-catalog = Catalog
view-mode-downloaded = Downloaded

date-limit-anytime = ANYTIME
date-limit-today = TODAY
date-limit-days3 = LAST 3 DAYS
date-limit-days7 = LAST 7 DAYS
date-limit-days14 = LAST 14 DAYS
date-limit-days30 = LAST 30 DAYS
date-limit-days90 = LAST 90 DAYS
date-limit-days180 = LAST 180 DAYS
date-limit-days365 = LAST 365 DAYS

tag-logic-or = OR
tag-logic-and = AND

search-mode-creator = CREATOR
search-mode-title = TITLE

# Common buttons
common-logs = Logs
common-about = About
common-settings = Settings
common-bookmarks = Bookmarks


# Settings extra
settings-startup-tags = Startup tags (added on app start):
settings-startup-tags-placeholder = Select a startup tag...
settings-startup-exclude-tags = Startup exclude tags (excluded on app start):
settings-startup-exclude-tags-placeholder = Select a tag to exclude at startup...
settings-startup-prefixes = Startup prefixes (included on app start):
settings-startup-prefixes-placeholder = Select a prefix to include at startup...
settings-startup-exclude-prefixes = Startup exclude prefixes (excluded on app start):
settings-startup-exclude-prefixes-placeholder = Select a prefix to exclude at startup...
settings-warn-heading = Warn on tags/prefixes:
settings-warn-tags = Warn tags:
settings-warn-tags-placeholder = Select a tag to warn...
settings-warn-prefixes = Warn prefixes:
settings-warn-prefixes-placeholder = Select a prefix to warn...

# Logs window
logs-window-title = Logs
logs-clear = Clear
logs-copy = Copy
logs-autoscroll = Autoscroll
logs-lines = { $n } lines

# Settings move confirm/progress
settings-move-confirm-title = Confirm folder change
settings-move-confirm-text = Installed games detected. All games will be moved to the new folder. Continue?
settings-move-confirm-move = Move
settings-move-progress-title = Moving installed games
settings-move-progress-text = Moving games… Do not close the app.

# Game updates
settings-check-updates = Check Updates
settings-update-all = Update All
settings-update-frequency = Update check frequency
settings-update-manual = Manual only
settings-update-on-startup = On startup
settings-update-every-n-days = Every { $days } days
settings-checking-updates = Checking updates...
card-update-available = UPDATE

card-context-bookmarks = 🔖 Bookmarks...

bookmarks-selector-title = Game Bookmarks
bookmarks-selector-add-placeholder = Add bookmark...
bookmarks-selector-create-new = + Create new
bookmarks-selector-emoji-placeholder = 🔖
bookmarks-selector-label-placeholder = Label
bookmarks-selector-create-btn = Create
bookmarks-selector-cancel-btn = Cancel

bookmarks-mgmt-title = Manage Bookmarks
bookmarks-mgmt-add-btn = + Add Bookmark
bookmarks-mgmt-edit-btn = Edit
bookmarks-mgmt-delete-btn = Delete
bookmarks-mgmt-delete-confirm = Are you sure you want to delete this bookmark? It will be removed from all games.
bookmarks-mgmt-save-btn = Save
bookmarks-mgmt-default-color = Default bookmark color:
bookmarks-mgmt-visible-limit = Bookmarks visible on cover:
bookmarks-mgmt-no-bookmarks = No bookmarks created yet.
errors-title = Errors
errors-clear = Clear
download-select-link = Select download link
bookmarks-new = New bookmark
bookmarks-edit = Edit bookmark
bookmarks-create = Create
bookmarks-emoji = Emoji
bookmarks-name = Name
bookmarks-color = Color

# Slint interface
catalog-empty = Catalog is empty
catalog-loading = Loading catalog…
catalog-load-error = Failed to load catalog: { $err }
library-empty = Library is empty
library-loading = Loading library…
library-installed-count = Installed games: { $count }
common-refresh = Refresh
common-hide = Hide
common-open-f95 = Open in F95
common-delete = Delete
common-open-folder = Open folder
context-remove-library = Remove from Library
common-saved = Saved
common-choose = Choose…
delete-game-title = Delete the game and its files?
delete-game-warning = This action cannot be undone.
about-window-title = About
about-version = Version { $version } · Slint interface
about-description = Native F95Zone catalog and library client.\n\nThe interface works without a browser engine. Slint uses retained layout and a native window.
about-footer = Made for convenient browsing, downloading, and launching games.
settings-folders = Folders
settings-interface = Interface
settings-ui-scale = UI SCALE
settings-card-scale = CARD SCALE
settings-image-cache-games = SCREENSHOT CACHE IN RAM (GAMES)
settings-choose-temp = TEMP DOWNLOADS
settings-choose-games = GAMES FOLDER
settings-choose-cache = CACHE FOLDER
settings-cover-metadata = COVERS AND METADATA
settings-loading-bottom = Bottom bar
settings-loading-circle = Circle at bottom right
settings-update-every-7-days = Every 7 days
settings-custom-warnings = Custom warnings
settings-add-tags = ADD TAGS
settings-exclude-tags = EXCLUDE TAGS
settings-add-prefixes = ADD PREFIXES
settings-exclude-prefixes = EXCLUDE PREFIXES
settings-max-10 = MAX. 10
settings-warning-tag-placeholder = Warning tag
settings-warning-prefix-placeholder = Warning prefix
settings-startup-filters = Startup filters
settings-bookmark-cover-count = BOOKMARK BADGES ON COVER
settings-bookmark-color = DEFAULT BOOKMARK COLOR
settings-color-red = RED
settings-color-green = GREEN
settings-color-blue = BLUE

