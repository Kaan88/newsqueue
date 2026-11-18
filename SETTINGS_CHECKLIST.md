# Settings checklist

Generated 2026-11-18 from `Settings` in `app/core/src/model.rs`: where each
setting is edited (the Setup page from the `FIELDS` table in `app/src/setup.rs`,
or another dialog of the native app) and which files read it. Private settings
(never sent or changed through the server API) are marked *private*. Re-check
after adding a setting: it needs a field, a default, a control and behaviour.

| Setting | Edited in | Read by |
|---|---|---|
| `download_dir` | Setup: Download | core/src/command.rs, core/src/download.rs, core/src/engine.rs, core/src/model.rs, server/src/lib.rs, src/main.rs, src/setup.rs, web/src/settings.rs |
| `nzb_dir` | Setup: NZB | core/src/engine.rs, core/src/model.rs, core/src/rss.rs, src/cli.rs, src/main.rs, src/setup.rs, web/src/settings.rs |
| `temp_dir` | Setup: Download | core/src/command.rs, core/src/download.rs, core/src/engine_tests.rs, core/src/model.rs, core/src/par2.rs, core/src/unrar.rs, server/src/lib.rs, src/main.rs, src/setup.rs, src/update.rs, web/src/settings.rs |
| `unrar_dir` | Setup: Download | core/src/engine_tests.rs, core/src/model.rs, core/src/unrar.rs, src/main.rs, src/setup.rs, web/src/settings.rs |
| `max_speed_kbps` | Setup: Connection | core/src/engine.rs, src/main.rs, src/setup.rs |
| `max_speed_mbps` | Setup: Connection (follows the line speed) | core/src/engine.rs, src/main.rs, src/setup.rs, web/src/settings.rs |
| `retry_count` | Setup: Download #2 | core/src/download.rs, core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `retry_delay` | Setup: Download #2 | core/src/download.rs, core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `low_free_space_mb` | Setup: Download | core/src/command.rs, core/src/engine.rs, server/src/lib.rs, src/setup.rs, web/src/settings.rs |
| `autosave_minutes` | Setup: Download | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `skip_existing` | Setup: Download | core/src/download.rs, core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `dblclick_expand` | Setup: Download | src/main.rs, src/setup.rs |
| `minimize_to_tray` | Setup: Miscellaneous | src/main.rs, src/setup.rs |
| `minimize_to_tray_on_close` | Setup: Miscellaneous | src/main.rs, src/setup.rs |
| `tray_balloon` | Setup: Miscellaneous | src/main.rs, src/setup.rs, src/tray.rs |
| `speed_in_tray` | Setup: Miscellaneous | src/main.rs, src/setup.rs, src/tray.rs |
| `ui_theme` | — | unused; kept so older settings files load |
| `check_version_minutes` | Setup: Miscellaneous | src/dialogs.rs, src/setup.rs |
| `auto_update` | Setup: Miscellaneous | src/dialogs.rs, src/setup.rs |
| `clear_history_on_exit` | Setup: Log | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `log_enabled` | Setup: Log | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `log_to_file` | Setup: Log | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `keep_log_days` | Setup: Log | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `max_log_lines` | Setup: Log | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `detailed_log` | Setup: Log | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `par2_checking` | Setup: Par2 | core/src/command.rs, core/src/engine.rs, core/src/engine_tests.rs, server/src/lib.rs, src/setup.rs, web/src/settings.rs |
| `par2_repairing` | Setup: Par2 | core/src/engine.rs, core/src/engine_tests.rs, core/src/par2.rs, src/setup.rs, web/src/settings.rs |
| `cleanup_after_repair` | Setup: Par2 | core/src/par2.rs, src/setup.rs, web/src/settings.rs |
| `par2_on_demand` | Setup: Par2 | core/src/command.rs, core/src/engine.rs, core/src/model.rs, src/setup.rs, web/src/settings.rs |
| `abort_hopeless` | Setup: Download #2 | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `precheck_articles` | Setup: Download #2 | core/src/download.rs, core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `direct_unpack` | Setup: Unpack | core/src/command.rs, core/src/engine.rs, server/src/lib.rs, src/setup.rs, web/src/settings.rs |
| `duplicate_check` | Setup: Download #2 | core/src/command.rs, core/src/engine.rs, server/src/lib.rs, src/setup.rs, web/src/settings.rs |
| `notify_url` *private* | Setup: Miscellaneous #3 | core/src/engine.rs, core/src/model.rs, server/src/lib.rs, src/setup.rs |
| `web_access` *private* | Setup: Miscellaneous #3 | core/src/model.rs, src/main.rs, src/setup.rs |
| `web_access_bind` *private* | Setup: Miscellaneous #3 | core/src/model.rs, src/main.rs, src/setup.rs |
| `web_access_key` *private* | Setup: Miscellaneous #3 | core/src/model.rs, src/main.rs, src/setup.rs |
| `sort_folder` | Setup: Miscellaneous #2 | core/src/command.rs, core/src/engine.rs, core/src/model.rs, src/setup.rs, web/src/settings.rs |
| `sort_pattern` | Setup: Miscellaneous #2 | core/src/command.rs, core/src/engine.rs, core/src/model.rs, src/setup.rs, web/src/settings.rs |
| `auto_unrar` | Setup: Unpack | core/src/command.rs, core/src/engine.rs, core/src/engine_tests.rs, server/src/lib.rs, src/setup.rs, web/src/settings.rs |
| `create_dir_nzb_name` | Setup: Unpack | core/src/engine_tests.rs, core/src/unrar.rs, src/setup.rs, web/src/settings.rs |
| `create_dir_group_name` | Setup: Unpack | core/src/engine_tests.rs, core/src/unrar.rs, src/setup.rs, web/src/settings.rs |
| `create_dir_archive_name` | Setup: Unpack | core/src/engine_tests.rs, core/src/unrar.rs, src/setup.rs, web/src/settings.rs |
| `auto_rename_to_collection` | Setup: Unpack | core/src/engine_tests.rs, core/src/unrar.rs, src/setup.rs, web/src/settings.rs |
| `delete_empty_folder` | Setup: Unpack | core/src/engine_tests.rs, core/src/par2.rs, core/src/unrar.rs, src/cli.rs, src/setup.rs, web/src/settings.rs |
| `pause_while_unraring` | Setup: Unpack | core/src/engine.rs, core/src/engine_tests.rs, src/setup.rs, web/src/settings.rs |
| `move_files_to_unrar` | Setup: Unpack | core/src/unrar.rs, src/setup.rs, web/src/settings.rs |
| `delete_files_after_unrar` | Setup: Unpack | core/src/unrar.rs, src/setup.rs, web/src/settings.rs |
| `overwrite_existing` | Setup: Unpack | core/src/engine_tests.rs, core/src/unrar.rs, src/setup.rs, web/src/settings.rs |
| `delete_archives` | Setup: Unpack | core/src/engine.rs, core/src/engine_tests.rs, core/src/model.rs, core/src/par2.rs, core/src/snapshot.rs, core/src/unrar.rs, src/cli.rs, src/queue.rs, src/setup.rs, web/src/settings.rs |
| `strip_root` | Setup: Unpack | core/src/engine_tests.rs, core/src/par2.rs, core/src/unrar.rs, src/cli.rs, src/setup.rs, web/src/settings.rs |
| `rename_existing` | Setup: Unpack | core/src/par2.rs, core/src/unrar.rs, src/cli.rs, src/setup.rs, web/src/settings.rs |
| `unrar_to_different` | Setup: Unpack | core/src/engine_tests.rs, core/src/unrar.rs, src/setup.rs |
| `proxy_enabled` | Setup: Proxy | core/src/model.rs, src/setup.rs, web/src/settings.rs |
| `proxy_address` | Setup: Proxy | core/src/model.rs, src/setup.rs, web/src/settings.rs |
| `proxy_port` | Setup: Proxy | core/src/download.rs, core/src/model.rs, src/setup.rs, web/src/settings.rs |
| `proxy_kind` | Setup: Proxy | core/src/model.rs, src/setup.rs, web/src/settings.rs |
| `proxy_auth` | Setup: Proxy | core/src/model.rs, src/setup.rs, web/src/settings.rs |
| `proxy_username` *private* | Setup: Proxy | core/src/model.rs, src/setup.rs |
| `proxy_password` *private* | Setup: Proxy | core/src/model.rs, server/src/lib.rs, src/setup.rs |
| `queue_bytes_mode` | Setup: User interface | src/main.rs, src/setup.rs |
| `speed_bytes_mode` | Setup: User interface | core/src/engine.rs, src/dialogs.rs, src/main.rs, src/setup.rs |
| `transfered_bytes_mode` | — | unused; kept so older settings files load |
| `thread_list_height` | Setup: User interface, splitter drag | src/main.rs, src/setup.rs |
| `thread_list_visible` | Setup: User interface, F9 / toolbar | src/main.rs, src/setup.rs |
| `show_tabs` | — | unused; kept so older settings files load |
| `column_visible` | Columns dialog | src/dialogs.rs, src/main.rs |
| `column_widths` | Columns dialog | src/dialogs.rs, src/main.rs |
| `servers` *private* | Setup: Servers (server editor) | core/src/command.rs, core/src/download.rs, core/src/engine.rs, core/src/model.rs, core/src/snapshot.rs, server/src/lib.rs, src/dialogs.rs, src/main.rs, src/setup.rs, web/src/net.rs, web/src/settings.rs |
| `notes` | Notes dialog | core/src/par2.rs, src/dialogs.rs, src/form.rs, src/main.rs, src/util.rs, web/src/skin.rs |
| `scheduler` | Setup: Scheduler (grid) | core/src/engine.rs, core/src/model.rs, src/main.rs, src/setup.rs, src/util.rs, web/src/settings.rs |
| `scheduler_enabled` | Setup: Scheduler | core/src/model.rs, src/main.rs, src/setup.rs, web/src/settings.rs |
| `shutdown_method` | Setup: Miscellaneous #3, Shutdown dialog | src/dialogs.rs, src/main.rs, src/setup.rs |
| `auto_import_folder` | Setup: NZB | core/src/engine.rs, core/src/engine_tests.rs, src/setup.rs, web/src/settings.rs |
| `delete_nzb_after_import` | Setup: NZB | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `auto_import_paused` | Setup: NZB | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `case_insensitive_filters` | Setup: NZB #2 | core/src/unrar.rs, src/setup.rs, web/src/settings.rs |
| `rss_refresh_minutes` | Setup: Miscellaneous #3 | core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `rss_urls` *private* | RSS feeds dialog | core/src/command.rs, core/src/engine.rs, core/src/model.rs, core/src/snapshot.rs, server/src/lib.rs, src/cli.rs, src/dialogs.rs |
| `rss_filters` | RSS feeds dialog | core/src/engine.rs, src/cli.rs, src/dialogs.rs, web/src/settings.rs |
| `default_execute` | Setup: NZB #2 | core/src/engine.rs, core/src/engine_tests.rs, src/setup.rs, web/src/settings.rs |
| `hide_execute_window` | Setup: NZB #2 | core/src/engine.rs, src/setup.rs |
| `article_cache_mb` | Setup: Miscellaneous #2 | core/src/download.rs, core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `cache_cleanup_minutes` | Setup: Miscellaneous #2 | core/src/download.rs, core/src/engine.rs, src/setup.rs, web/src/settings.rs |
| `download_priority` | Setup: Miscellaneous #2 | core/src/download.rs, core/src/engine.rs, src/setup.rs |
| `exp_folders_visible` | — | unused; kept so older settings files load |
| `exp_folders_width` | — | unused; kept so older settings files load |
| `startup_connect` | Setup: Connection | src/main.rs, src/setup.rs, web/src/settings.rs |
| `startup_speed_limit` | Setup: Connection | core/src/engine.rs, src/setup.rs |
| `unrar_path` | Setup: Unpack | core/src/engine.rs, core/src/tools.rs, src/setup.rs, web/src/settings.rs |
| `par2_path` | Setup: Par2 | core/src/engine.rs, core/src/tools.rs, src/setup.rs, web/src/settings.rs |
| `sevenzip_path` | Setup: Unpack | core/src/engine.rs, core/src/tools.rs, src/setup.rs, web/src/settings.rs |
