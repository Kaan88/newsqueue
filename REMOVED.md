# Removed features

These parts of AltBinz 0.48.4 were cut from NewsQueue on purpose. Old settings
files still load; the removed keys are ignored.

## 2026-11-18 — global search
- Global/Newznab search, the toolbar Search button, Ctrl+F, and the search settings.

## 2026-11-18 — rarely used features
- **Explorer tab**, with its folder tree, file list, history, rename, delete,
  open-with, drag/drop, clipboard, columns and favourites. Also removed: the
  `NQ_DEMO_EXPLORER` demo hook and the dropped-files handler. Tab index 2 is
  left empty so saved tab layouts keep working.
- **File join/split.** This removed `files.rs`, the queue Join/Split buttons and
  menu items, and the split dialog. Archives are extracted, not joined.
- **NZB Play**, with its setup page, the Play button and `player_exe`.
- **Categories**, with the categories dialog, "Set category",
  `Collection.category` and keyword/folder routing.
- **Forum and Donate** toolbar buttons and their icons.
- **Stubs**:
  - the "Set unrar password" button
  - the duplicate "Export selected to multiple NZBs" button
  - the "Set password" item in the context menu
  - the "Clear history" button
  - the Language combo
- **The "User interface #2" setup page**, which became empty. Setup now has 15 pages.
- **40 settings** that were stored but never read: connections_per_thread,
  connection_retry_delay, connection_timeout, xpat_timeout,
  connected_while_idle, grace_period, max_path_length, cleanup_queue,
  prompt_for_name, progress_taskbar, status_xml, download_history,
  speed_and_bytes, delete_small_par2, save_par2_info,
  max_connections_per_server, max_servers_per_level, show_captions, merge_nzb,
  sort_nzb, sort_rars, add_to_top, pause_nzb, pause_samples, pause_par2,
  autofocus, alternate_rename, disable_undocking, tray_single_click, keep_nfo,
  auto_cleanup_minutes, delete_rars_after_extract, decoder_sleep_lines,
  decode_priority, par2_priority, show_images, small_images, startup_pause,
  startup_scheduler. The "Pause downloading" setup groups went with them.
- **Constants**: MAX_SERVERS_PER_LEVEL, CONNECTION_RETRY_DELAY, XPAT_TIMEOUT,
  MAX_PATH_LENGTH and EXPLORER_COLUMN_NAMES.
- **6 tests** that covered Explorer copy/paste and file join/split.

## Fixed in the same round, not removed
- **Home** opens the project page.
- **To tray** now really sends the window to the tray, through the off-screen
  path.
- **Version?** checks GitHub releases. It reports when a new version is out and,
  with "Install new versions automatically", downloads the setup, checks its
  SHA-256, installs it silently and relaunches when the queue is idle.
  `check_version_minutes` sets how often it checks (0 = never).
- **The "Change unrar folder" button** used to change the download folder. It
  now changes the unrar folder.
