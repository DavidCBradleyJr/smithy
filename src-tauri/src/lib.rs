mod agents;
mod local;
mod runtime;
mod settings;
mod theme;
mod vram;

use tauri::Manager as _;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(agents::Manager::new())
        .setup(|app| {
            theme::watch(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            theme::get_theme,
            vram::get_vram,
            runtime::runtime_status,
            runtime::runtime_start,
            runtime::runtime_stop,
            settings::settings_get,
            settings::settings_set,
            local::local_state,
            local::local_load,
            local::local_unload,
            agents::agents_list,
            agents::projects_list,
            agents::project_add,
            agents::project_remove,
            agents::project_branch,
            agents::threads_list,
            agents::thread_live,
            agents::thread_create,
            agents::thread_history,
            agents::thread_delete,
            agents::thread_prompt,
            agents::thread_cancel,
            agents::thread_set_config,
            agents::permission_respond,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<agents::Manager>().shutdown();
            }
        });
}
