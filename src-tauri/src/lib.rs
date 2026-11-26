mod activity;
mod audio;
mod classify;
mod commands;
mod db;
mod detect;
mod error;
mod import_v1;
mod notes;
mod recorder;
mod settings;
mod shortcuts;
mod storage;
mod timesheet;
mod tracker;
mod transcribe;
mod tray;
mod windows;

use tauri::{Manager, RunEvent, WindowEvent};
use tracing_subscriber::EnvFilter;

const RECORDINGS_DIR: &str = "recordings";
const MODELS_DIR: &str = "models";

pub fn run() {
    init_tracing();

    tauri::Builder::default()
        // Must be first: a second launch focuses the running app instead of starting again.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(shortcuts::plugin())
        .manage(recorder::RecorderHandle::default())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            storage::apply_staged_restore(&data_dir);
            let db_path = data_dir.join(storage::DB_FILE);
            let db = db::Db::open(&db_path)?;
            tracing::info!(path = %db_path.display(), version = db.schema_version()?, "database ready");
            match db.recover_interrupted() {
                Ok(0) => {}
                Ok(n) => tracing::warn!(count = n, "recovered recordings interrupted by a crash"),
                Err(err) => tracing::error!(%err, "could not recover interrupted recordings"),
            }
            if let Err(err) = db.requeue_interrupted_transcriptions() {
                tracing::error!(%err, "could not requeue interrupted transcriptions");
            }
            app.manage(db);
            app.manage(storage::RecordingPaths {
                default_root: data_dir.join(RECORDINGS_DIR),
            });
            storage::allow_playback(app.handle());
            app.manage(transcribe::Transcriber::start(
                app.handle().clone(),
                data_dir.join(MODELS_DIR),
            ));
            app.manage(commands::transcribe::Downloads::default());
            app.manage(tracker::Tracker::start(
                app.handle().clone(),
                app.path().app_cache_dir()?.join("browser-history"),
            ));

            tray::create(app.handle())?;
            shortcuts::register(app.handle());
            windows::place_widget(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing a window hides it. The app keeps running in the tray. Quit from the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(err) = window.hide() {
                    tracing::warn!(%err, label = window.label(), "could not hide window");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::recorder_state,
            commands::recorder_start,
            commands::recorder_stop,
            commands::recorder_pause,
            commands::recorder_resume,
            commands::show_main_window,
            commands::app_info,
            commands::meetings::list_meetings,
            commands::meetings::get_meeting,
            commands::meetings::rename_meeting,
            commands::meetings::delete_meeting,
            commands::meetings::retranscribe_meeting,
            commands::meetings::search_transcripts,
            commands::meetings::reveal_meeting,
            commands::transcribe::list_models,
            commands::transcribe::download_model,
            commands::transcribe::delete_model,
            commands::transcribe::transcription_status,
            commands::notes::get_notes,
            commands::notes::generate_notes,
            commands::notes::save_notes,
            commands::notes::set_notes_key,
            commands::notes::has_notes_key,
            commands::notes::day_action_items,
            commands::notes::check_notes_provider,
            commands::data::backup_now,
            commands::data::restore_backup,
            commands::data::import_v1_status,
            commands::data::run_import_v1,
            commands::data::recordings_folder,
            commands::work::tracking_status,
            commands::work::request_tracking_permission,
            commands::work::day_report,
            commands::work::range_report,
            commands::work::delete_day_activity,
            commands::work::list_projects,
            commands::work::save_project,
            commands::work::delete_project,
            commands::work::get_timesheet,
            commands::work::save_timesheet,
            commands::work::reset_timesheet,
            commands::work::export_timesheet,
            commands::get_setting,
            commands::set_setting,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TaskMind")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                recorder::shutdown(app);
                if let Some(t) = app.try_state::<transcribe::Transcriber>() {
                    t.shutdown();
                }
                if let Some(t) = app.try_state::<tracker::Tracker>() {
                    t.shutdown();
                }
            }
        });
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,taskmind_lib=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
