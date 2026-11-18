# Settings checklist

Generated 2026-11-18 from `Settings` in `app/core/src/model.rs`: where each
setting is edited (Setup page function in `app/src/dialogs.rs`, or another
control) and which files read it. Private settings (never sent by the
server API) are marked *private*. Re-check after adding a setting: it needs
a field, a default, a control and behaviour.

| Setting | Edited in | Read by |
|---|---|---|
| `download_dir` | page_download | command.rs, download.rs, engine.rs, main.rs, model.rs, ui.rs |
| `nzb_dir` | page_download, page_nzb | engine.rs, main.rs, model.rs |
| `temp_dir` | page_download | model.rs |
| `unrar_dir` | page_download, page_unpack | main.rs, model.rs, ui.rs, unrar.rs |
| `max_speed_kbps` | page_connection | engine.rs |
| `max_speed_mbps` | page_connection | engine.rs |
| `retry_count` | page_connection | download.rs, engine.rs |
| `retry_delay` | page_connection | download.rs, engine.rs |
| `low_free_space_mb` | page_download | command.rs, engine.rs |
| `autosave_minutes` | page_download | engine.rs |
| `skip_existing` | page_download | download.rs, engine.rs |
| `dblclick_expand` | page_download | ui.rs |
| `minimize_to_tray` | page_misc | main.rs |
| `minimize_to_tray_on_close` | page_misc | main.rs |
| `tray_balloon` | page_misc | main.rs |
| `speed_in_tray` | page_misc | main.rs |
| `ui_theme` | page_ui | main.rs |
| `check_version_minutes` | page_misc | main.rs |
| `auto_update` | page_misc | main.rs |
| `clear_history_on_exit` | page_misc | engine.rs, main.rs |
| `log_enabled` | page_log | engine.rs |
| `log_to_file` | page_log | engine.rs |
| `keep_log_days` | page_log | engine.rs |
| `max_log_lines` | page_log | engine.rs |
| `detailed_log` | page_log | engine.rs |
| `par2_checking` | page_download, page_par2 | command.rs, engine.rs, main.rs |
| `par2_repairing` | page_par2 | engine.rs, main.rs, par2.rs |
| `cleanup_after_repair` | page_par2 | par2.rs |
| `par2_on_demand` | page_par2 | command.rs, engine.rs |
| `abort_hopeless` | page_par2 | engine.rs |
| `precheck_articles` | page_par2 | engine.rs |
| `direct_unpack` | page_par2 | command.rs, engine.rs, main.rs |
| `duplicate_check` | page_misc3 | command.rs, engine.rs, main.rs |
| `notify_url` *private* | page_misc3 | engine.rs, main.rs |
| `sort_folder` | page_unpack | command.rs, engine.rs |
| `sort_pattern` | page_unpack | command.rs, engine.rs |
| `auto_unrar` | page_unpack | command.rs, engine.rs, main.rs |
| `create_dir_nzb_name` | page_unpack | main.rs, unrar.rs |
| `create_dir_group_name` | page_unpack | main.rs, unrar.rs |
| `create_dir_archive_name` | page_unpack | main.rs, unrar.rs |
| `auto_rename_to_collection` | page_unpack | main.rs, unrar.rs |
| `delete_empty_folder` | page_unpack | main.rs, unrar.rs |
| `pause_while_unraring` | page_unpack | engine.rs, main.rs |
| `move_files_to_unrar` | page_unpack | unrar.rs |
| `delete_files_after_unrar` | page_unpack | unrar.rs |
| `overwrite_existing` | page_unpack | main.rs, unrar.rs |
| `delete_archives` | page_unpack | main.rs, ui.rs, unrar.rs |
| `strip_root` | page_unpack | main.rs, unrar.rs |
| `rename_existing` | page_unpack | unrar.rs |
| `unrar_to_different` | page_unpack | main.rs, unrar.rs |
| `proxy_enabled` | page_proxy | model.rs |
| `proxy_address` | page_proxy | model.rs |
| `proxy_port` | page_proxy | model.rs |
| `proxy_kind` | page_proxy | model.rs |
| `proxy_auth` | page_proxy | model.rs |
| `proxy_username` *private* | page_proxy | model.rs |
| `proxy_password` *private* | page_proxy | model.rs |
| `queue_bytes_mode` | setup_window | main.rs, ui.rs |
| `speed_bytes_mode` | setup_window | engine.rs, main.rs, ui.rs |
| `transfered_bytes_mode` | setup_window | main.rs, ui.rs |
| `thread_list_height` | page_ui, setup_window | main.rs, ui.rs |
| `thread_list_visible` | page_ui, setup_window | main.rs, ui.rs |
| `show_tabs` | page_ui, setup_window | ui.rs |
| `column_visible` | columns_window, setup_window | main.rs, ui.rs |
| `column_widths` | columns_window, setup_window | main.rs, ui.rs |
| `servers` *private* | page_servers, server_edit_window | command.rs, download.rs, engine.rs, main.rs, model.rs |
| `notes` | notes_window | main.rs, ui.rs |
| `scheduler` | page_scheduler | model.rs |
| `scheduler_enabled` | page_scheduler | model.rs, ui.rs |
| `shutdown_method` | shutdown_window | main.rs |
| `auto_import_folder` | page_nzb | engine.rs, main.rs |
| `delete_nzb_after_import` | page_download2, page_nzb, page_nzb2 | engine.rs, main.rs |
| `auto_import_paused` | page_download2, page_nzb | engine.rs |
| `case_insensitive_filters` | page_download2, page_nzb2 | unrar.rs |
| `rss_refresh_minutes` | page_misc3 | engine.rs |
| `rss_urls` *private* | rss_feeds_window | engine.rs, main.rs, ui.rs |
| `rss_filters` | rss_feeds_window | engine.rs, main.rs |
| `default_execute` | page_misc | engine.rs, main.rs |
| `hide_execute_window` | page_misc | engine.rs |
| `article_cache_mb` | page_misc2 | download.rs, engine.rs |
| `cache_cleanup_minutes` | page_misc2 | download.rs, engine.rs |
| `download_priority` | page_misc2 | download.rs, engine.rs |
| `exp_folders_visible` | — | — (Explorer tab removed; kept for old files) |
| `exp_folders_width` | — | — (Explorer tab removed; kept for old files) |
| `startup_connect` | page_misc | main.rs |
| `startup_speed_limit` | page_scheduler | engine.rs |

86 settings.
