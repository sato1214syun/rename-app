use chrono::{DateTime, Utc};
use std::fs;
use std::path::PathBuf;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    name: String,
    path: PathBuf,
    modified: DateTime<Utc>,
    new_name: Option<String>, // Add this field
}

// リネーム用の専用構造体
#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RenameFileEntry {
    name: String,
    path: PathBuf,
    modified: DateTime<Utc>,
    new_name: String, // 必須フィールド
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
fn rename_files(files: Vec<RenameFileEntry>) -> Result<(), String> {
    println!("rename_files called with {} files", files.len());

    for (index, file) in files.iter().enumerate() {
        println!(
            "Processing file {}: name='{}', new_name='{}'",
            index, file.name, file.new_name
        );

        if file.new_name.trim().is_empty() {
            let error_msg = format!("New name is empty for file: {}", file.name);
            println!("Error: {}", error_msg);
            return Err(error_msg);
        }

        let new_path = file.path.with_file_name(&file.new_name);
        println!(
            "Renaming '{}' to '{}'",
            file.path.display(),
            new_path.display()
        );

        match fs::rename(&file.path, &new_path) {
            Ok(_) => println!(
                "Successfully renamed '{}' to '{}'",
                file.name, file.new_name
            ),
            Err(e) => {
                let error_msg = format!(
                    "Failed to rename '{}' to '{}': {}",
                    file.name, file.new_name, e
                );
                println!("Error: {}", error_msg);
                return Err(error_msg);
            }
        }
    }

    println!("All files renamed successfully");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            read_files_in_directory,
            rename_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
