use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Utc};

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    name: String,
    path: PathBuf,
    modified: DateTime<Utc>,
    new_name: Option<String>, // Add this field
}

#[tauri::command]
fn read_files_in_directory(path: PathBuf) -> Result<Vec<FileEntry>, String> {
    let mut entries: Vec<FileEntry> = Vec::new();
    for entry_result in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry_result.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;

        if file_type.is_file() {
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            let modified: DateTime<Utc> = metadata.modified().map_err(|e| e.to_string())?.into();
            entries.push(FileEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                modified,
                new_name: None,
            });
        }
    }
    Ok(entries)
}

#[tauri::command]
fn rename_files(files: Vec<FileEntry>) -> Result<(), String> {
    for file in files {
        if let Some(new_name) = file.new_name {
            let new_path = file.path.with_file_name(new_name);
            fs::rename(&file.path, &new_path).map_err(|e| e.to_string())?;
        } else {
            return Err(format!("New name not provided for file: {}", file.name));
        }
    }
    Ok(())
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![read_files_in_directory, rename_files])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
