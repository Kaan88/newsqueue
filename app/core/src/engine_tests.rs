//! Engine tests moved from the egui desktop's `main.rs` (GUI_PLAN Phase 0
//! step 7): execute hooks, the unrar password round trip, auto-import, the
//! PAR2 -> unrar chain and statistics, all without a front end.

use crate::command::Command;
use crate::engine::*;
use crate::model::*;
use std::path::PathBuf;

#[test]
fn transfer_stats_reset_daily_on_a_new_day_and_monthly_on_a_new_year() {
    let mut stats = TransferStats {
        day: "2026-11-03".into(),
        year: 2026,
        daily: [1.0; 24],
        monthly: [2.0; 12],
        ..Default::default()
    };
    stats.normalize_for(2026, "2026-11-04");
    assert_eq!(stats.daily, [0.0; 24]);
    assert_eq!(stats.monthly, [2.0; 12]);

    stats.daily[7] = 9.0;
    stats.normalize_for(2026, "2026-11-04");
    assert_eq!(stats.daily[7], 9.0);

    stats.normalize_for(2027, "2027-01-01");
    assert_eq!(stats.monthly, [0.0; 12]);
}

#[test]
fn transfer_stats_round_trip_keeps_counters() {
    let stats = TransferStats {
        day: "2026-11-04".into(),
        year: 2026,
        daily: [1.5; 24],
        monthly: [2.5; 12],
        collections: 12,
        startups: 7,
        longest_uptime_secs: 5_400,
    };
    let text = serde_json::to_string(&stats).unwrap();
    let loaded: TransferStats = serde_json::from_str(&text).unwrap();
    assert_eq!(loaded.collections, 12);
    assert_eq!(loaded.startups, 7);
    assert_eq!(loaded.longest_uptime_secs, 5_400);
    assert_eq!(loaded.daily, [1.5; 24]);
    assert_eq!(loaded.monthly, [2.5; 12]);
}

#[test]
fn hook_placeholders_become_quoted_variables() {
    assert_eq!(
        hook_shell_command("echo {name}>x.txt", true),
        "echo \"%NQ_NAME%\">x.txt"
    );
    assert_eq!(
        hook_shell_command("copy \"{folder}\\a.txt\" {file}", true),
        "copy \"%NQ_FOLDER%\\a.txt\" \"%NQ_FILE%\""
    );
    assert_eq!(hook_shell_command("run {name}", false), "run \"${NQ_NAME}\"");
    assert_eq!(hook_env_value("a\"b\r\nc"), "abc");
}

#[cfg(target_os = "windows")]
#[test]
fn execute_hook_does_not_run_commands_from_the_nzb_name() {
    let root = std::env::temp_dir().join(format!("newsqueue-execute-inject-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut app = Engine::new(Settings::default(), true);
    app.queue.clear();
    let mut collection = sample_queue(&app.settings).remove(0);
    collection.id = 301;
    collection.folder = root.to_string_lossy().to_string();
    collection.subject = "safe & echo pwned>pwned.txt \" & echo two>pwned2.txt".into();
    collection.execute = "echo {name}>marker.txt".into();
    collection.execute_done = false;
    app.queue = vec![collection];
    app.run_execute_hook(301);
    for _ in 0..100 {
        if root.join("marker.txt").is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    let marker = std::fs::read_to_string(root.join("marker.txt")).unwrap();
    assert!(marker.contains("safe & echo pwned"), "{marker}");
    assert!(!root.join("pwned.txt").exists());
    assert!(!root.join("pwned2.txt").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(target_os = "windows")]
#[test]
fn execute_hooks_use_per_collection_and_default_commands() {
    let root = std::env::temp_dir().join(format!(
        "newsqueue-execute-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let first = root.join("first");
    let second = root.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let mut app = Engine::new(Settings::default(), true);
    app.queue.clear();
    let mut per_collection = sample_queue(&app.settings).remove(0);
    per_collection.id = 201;
    per_collection.folder = first.to_string_lossy().to_string();
    per_collection.execute = "cmd /C echo one>marker.txt".into();
    per_collection.execute_done = false;
    let mut defaulted = sample_queue(&app.settings).remove(0);
    defaulted.id = 202;
    defaulted.folder = second.to_string_lossy().to_string();
    defaulted.execute.clear();
    defaulted.execute_done = false;
    app.queue = vec![per_collection, defaulted];
    app.settings.default_execute = "cmd /C echo two>marker.txt".into();

    app.run_execute_hook(201);
    app.run_execute_hook(202);
    let one_ready = || {
        std::fs::read_to_string(first.join("marker.txt"))
            .map(|text| text.trim() == "one")
            .unwrap_or(false)
    };
    let two_ready = || {
        std::fs::read_to_string(second.join("marker.txt"))
            .map(|text| text.trim() == "two")
            .unwrap_or(false)
    };
    for _ in 0..100 {
        if one_ready() && two_ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
        std::fs::read_to_string(first.join("marker.txt")).unwrap().trim(),
        "one"
    );
    assert_eq!(
        std::fs::read_to_string(second.join("marker.txt")).unwrap().trim(),
        "two"
    );
    assert!(app.queue.iter().all(|collection| collection.execute_done));
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(target_os = "windows")]
#[test]
fn unrar_password_prompt_retries_and_extracts() {
    let root = std::env::temp_dir().join(format!(
        "newsqueue-unrar-prompt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let source = root.join("source");
    let dest = root.join("dest");
    std::fs::create_dir_all(&source).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("encrypted.rar");
    std::fs::copy(fixture, source.join("encrypted.rar")).unwrap();

    let mut app = Engine::new(Settings::default(), true);
    app.settings.strip_root = false;
    app.settings.auto_rename_to_collection = false;
    app.settings.delete_archives = false;
    app.settings.delete_empty_folder = false;
    app.settings.create_dir_archive_name = false;
    app.settings.create_dir_group_name = false;
    app.settings.create_dir_nzb_name = false;
    app.settings.pause_while_unraring = false;
    app.queue.clear();
    let mut collection = sample_queue(&app.settings).remove(0);
    collection.id = 77;
    collection.subject = "Password test".into();
    collection.folder = source.to_string_lossy().to_string();
    collection.unrar_folder = dest.to_string_lossy().to_string();
    collection.unrar = true;
    collection.password.clear();
    collection.status = ItemStatus::Completed;
    collection.files.clear();
    app.queue = vec![collection];

    assert!(app.start_unrar(77));
    let mut asked = false;
    for _ in 0..300 {
        app.poll_unrar();
        asked |= app.take_events().iter().any(|e| matches!(e, Event::PasswordNeeded(77)));
        if asked {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(asked, "no password event");
    app.apply(Command::SubmitPassword(77, "secret".into()));
    for _ in 0..300 {
        app.poll_unrar();
        if app.active_unrar.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(app.active_unrar.is_none(), "retry did not finish");
    assert_eq!(app.queue[0].status, ItemStatus::Completed);
    assert_eq!(
        std::fs::read_to_string(dest.join("payload.txt"))
            .unwrap()
            .trim(),
        "secret payload"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn auto_imports_nzbs_from_the_watch_folder() {
    let root = std::env::temp_dir().join(format!(
        "newsqueue-auto-import-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let xml = r#"<?xml version="1.0"?>
<nzb xmlns="http://www.newzbin.com/DTD/2003/nzb">
<file subject="&quot;watch-test.rar&quot; yEnc (1/1)" poster="tester" date="1700000000">
<groups><group>alt.binaries.test</group></groups>
<segments><segment bytes="10" number="1">watch@example</segment></segments>
</file>
</nzb>"#;
    std::fs::write(root.join("watch.nzb"), xml).unwrap();
    let mut app = Engine::new(Settings::default(), true);
    app.queue.clear();
    app.settings.auto_import_folder = root.to_string_lossy().to_string();
    app.auto_import_nzbs();
    assert_eq!(app.queue.len(), 1);
    assert_eq!(app.queue[0].subject, "watch");
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(target_os = "windows")]
#[test]
fn par2_repair_then_unrar_chain_works() {
    let root = std::env::temp_dir().join(format!(
        "newsqueue-chain-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let source = root.join("source");
    let dest = root.join("dest");
    std::fs::create_dir_all(&source).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("tiny.rar");
    let payload = source.join("payload.rar");
    std::fs::copy(fixture, &payload).unwrap();
    let par2_exe = app_base().join("tools").join("par2.exe");
    let output = std::process::Command::new(&par2_exe)
        .args(["c", "-q", "-r20", "payload.par2", "payload.rar"])
        .current_dir(&source)
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut damaged = std::fs::read(&payload).unwrap();
    let index = damaged.len() / 2;
    damaged[index] ^= 0xff;
    std::fs::write(&payload, damaged).unwrap();

    let mut app = Engine::new(Settings::default(), true);
    app.settings.auto_unrar = true;
    app.settings.par2_checking = true;
    app.settings.par2_repairing = true;
    app.settings.overwrite_existing = true;
    app.settings.unrar_to_different = true;
    app.settings.unrar_dir = dest.to_string_lossy().to_string();
    let mut collection = sample_queue(&app.settings).remove(0);
    collection.id = 77;
    collection.subject = "Chain test".into();
    collection.folder = source.to_string_lossy().to_string();
    collection.unrar = true;
    collection.unrar_folder.clear();
    collection.files.clear();
    collection.status = ItemStatus::Completed;
    app.queue = vec![collection];

    assert!(app.start_par2(77));
    for _ in 0..400 {
        app.poll_par2();
        app.poll_unrar();
        if app.active_par2.is_none() && app.active_unrar.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(app.active_par2.is_none(), "par2 did not finish");
    assert!(app.active_unrar.is_none(), "unrar did not finish");
    assert_eq!(app.queue[0].status, ItemStatus::Completed);
    assert_eq!(
        std::fs::read_to_string(dest.join("readme.txt"))
            .unwrap()
            .trim(),
        "tiny rar fixture"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(target_os = "windows")]
#[test]
fn completed_collection_can_be_unpacked() {
    let root = std::env::temp_dir().join(format!(
        "newsqueue-app-unrar-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let source = root.join("source");
    let dest = root.join("dest");
    std::fs::create_dir_all(&source).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("tiny.rar");
    std::fs::copy(fixture, source.join("tiny.rar")).unwrap();

    let mut app = Engine::new(Settings::default(), true);
    app.settings.unrar_to_different = true;
    app.settings.unrar_dir = dest.to_string_lossy().to_string();
    let mut collection = sample_queue(&app.settings).remove(0);
    collection.id = 99;
    collection.subject = "Tiny test".into();
    collection.folder = source.to_string_lossy().to_string();
    collection.unrar = true;
    collection.unrar_folder.clear();
    collection.files.clear();
    collection.status = ItemStatus::Completed;
    app.queue = vec![collection];

    assert!(app.start_unrar(99));
    for _ in 0..200 {
        app.poll_unrar();
        if app.active_unrar.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(app.active_unrar.is_none(), "unrar did not finish");
    assert_eq!(app.queue[0].status, ItemStatus::Completed);
    assert_eq!(
        std::fs::read_to_string(dest.join("readme.txt"))
            .unwrap()
            .trim(),
        "tiny rar fixture"
    );
    let _ = std::fs::remove_dir_all(root);
}
