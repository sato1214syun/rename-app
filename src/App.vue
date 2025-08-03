<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { message, open } from '@tauri-apps/plugin-dialog';
import { computed, ref } from "vue";

interface FileEntry {
  name: string;
  path: string;
  modified: string; // ISO 8601 string from Rust's DateTime<Utc>
  newName?: string; // Add this field
}

const files = ref<FileEntry[]>([]);
const currentDirectory = ref<string | null>(null);
const searchRegex = ref("");
const preserveExtension = ref(false);
const replaceText = ref("");
const errorMessage = ref("");

const sortKey = ref('name');
const sortOrder = ref('asc');

function _sortBy(key: string) {
  if (sortKey.value === key) {
    sortOrder.value = sortOrder.value === 'asc' ? 'desc' : 'asc';
  } else {
    sortKey.value = key;
    sortOrder.value = 'asc';
  }
}

async function _openDirectory() {
  try {
    const dir = await open({
      directory: true,
      multiple: false,
      title: 'Select a directory to rename files in'
    });

    if (typeof dir === 'string') {
      currentDirectory.value = dir;
      const result = await invoke("read_files_in_directory", { path: dir });

      if (Array.isArray(result)) {
        files.value = result as FileEntry[];
        errorMessage.value = "";
      } else {
        files.value = [];
        errorMessage.value = `Error reading directory contents: ${result}`;
      }
    }
  } catch (error) {
    errorMessage.value = `Error opening directory: ${error}`;
    files.value = [];
  }
}

const processedFiles = computed(() => {
  if (files.value.length === 0) {
    return [];
  }

  try {
    // ファイルを処理してnewNameを追加
    let processedFileList = files.value.map(file => ({
      ...file,
      newName: file.name, // 初期値は元のファイル名
      error: null as string | null
    }));

    if (!searchRegex.value && !replaceText.value) {
      return processedFileList;
    }

    const regex = searchRegex.value ? new RegExp(searchRegex.value) : null;
    const sequenceMatch = replaceText.value.match(/\{(\d+)\}/);
    const sequenceLength = sequenceMatch?.[1]?.length ?? 0;
    const startNumber = sequenceMatch?.[1] ? parseInt(sequenceMatch[1], 10) : 0;
    const sequencePlaceholder = sequenceMatch?.[0] ?? '';

    // 基本的な名前変更を適用
    processedFileList = processedFileList.map(file => {
      let baseName = file.name;
      let extension = '';
      if (preserveExtension.value) {
        const lastDotIndex = baseName.lastIndexOf('.');
        if (lastDotIndex > 0) {
          extension = baseName.substring(lastDotIndex);
          baseName = baseName.substring(0, lastDotIndex);
        }
      }
      const replacedBase = regex ? baseName.replace(regex, replaceText.value) : baseName;
      const newName = replacedBase + extension;
      // newNameが空やundefinedの場合はエラーをセット
      let error: string | null = null;
      if (!newName || newName.trim() === "") {
        error = "New name is empty";
      }
      return {
        ...file,
        newName,
        error
      };
    });

    // シーケンス番号を適用（必要な場合）
    if (sequenceMatch) {
      // ソートして順序を決定
      const sortedFiles = [...processedFileList].sort((a, b) => {
        const aVal = sortKey.value === 'name' ? a.name :
                    sortKey.value === 'newName' ? a.newName :
                    sortKey.value === 'modified' ? a.modified : a.name;
        const bVal = sortKey.value === 'name' ? b.name :
                    sortKey.value === 'newName' ? b.newName :
                    sortKey.value === 'modified' ? b.modified : b.name;

        const comparison = aVal.localeCompare(bVal);
        return sortOrder.value === 'asc' ? comparison : -comparison;
      });

      let sequenceCounter = 0;
      const fileIndexMap = new Map(processedFileList.map((file, index) => [file.path, index]));

      sortedFiles.forEach(sortedFile => {
        const originalIndex = fileIndexMap.get(sortedFile.path);
        if (originalIndex !== undefined) {
          const file = processedFileList[originalIndex];
          if (file.name !== file.newName) { // 変更されたファイルのみ
            const paddedNumber = String(startNumber + sequenceCounter).padStart(sequenceLength, '0');
            file.newName = file.newName.replace(sequencePlaceholder, paddedNumber);
            sequenceCounter++;
          }
        }
      });
    }

    return processedFileList;

  } catch (e: unknown) {
    const errorMessage = e instanceof Error ? e.message : String(e);
    return files.value.map(file => ({
      ...file,
      newName: file.name,
      error: errorMessage
    }));
  }
});

const _sortedRenamedFiles = computed(() => {
  const processedFileList = processedFiles.value;
  if (!processedFileList || processedFileList.length === 0) {
    return [];
  }

  try {
    const sorted = [...processedFileList].sort((a, b) => {
      const aVal = sortKey.value === 'name' ? a.name :
                  sortKey.value === 'newName' ? a.newName :
                  sortKey.value === 'modified' ? a.modified : a.name;
      const bVal = sortKey.value === 'name' ? b.name :
                  sortKey.value === 'newName' ? b.newName :
                  sortKey.value === 'modified' ? b.modified : b.name;

      const comparison = aVal.localeCompare(bVal);
      return sortOrder.value === 'asc' ? comparison : -comparison;
    });

    return sorted;
  } catch {
    return [];
  }
});

async function _rename() {
  try {
    const processedFileList = processedFiles.value;
    if (!processedFileList || processedFileList.length === 0) return;

    // 変更されたファイルをフィルタリング（エラーがないもののみ）
    const filesToRename = processedFileList.filter(f =>
      f.name !== f.newName &&
      !f.error &&
      f.newName &&
      f.newName.trim() !== ''
    );

    const filesToRenamePayload = filesToRename.map(f => ({
      name: f.name,
      path: f.path,
      modified: f.modified,
      newName: f.newName
    })).filter(payload => payload.newName && payload.newName.trim() !== '');

    // 最終的にnewNameが空のアイテムがないかチェック
    const invalidPayloads = filesToRenamePayload.filter(p => !p.newName || p.newName.trim() === '');
    if (invalidPayloads.length > 0) {
      errorMessage.value = `Invalid new names found for files: ${invalidPayloads.map(p => p.name).join(', ')}`;
      return;
    }

    if (filesToRenamePayload.length === 0) {
      return;
    }

    // Check for empty new names
    const emptyNames = processedFileList.filter(f => !f.newName || f.newName.trim() === '');
    if (emptyNames.length > 0) {
      await message('One or more files would be renamed to an empty name. Please adjust your regex or replacement text.', {
        title: 'Invalid Rename',
        kind: 'error',
      });
      return;
    }

    const extensionOnlyNames = processedFileList.filter(f => {
      // Check if the new name is just an extension (e.g., ".txt") or just "."
      return f.newName?.startsWith('.') && f.newName.length > 1 && f.newName.substring(1).indexOf('.') === -1 && f.newName.substring(1).length > 0;
    });

    if (extensionOnlyNames.length > 0) {
      await message('One or more files would be renamed to an extension only (e.g., ".txt"). Please adjust your regex or replacement text.', {
        title: 'Invalid Rename',
        kind: 'error',
      });
      return;
    }

    // Check for duplicate new names
    const newNames = processedFileList.map(f => f.newName).filter((name): name is string => name !== undefined);
    const seenNames = new Set<string>();
    const duplicateNames: string[] = [];

    for (const name of newNames) {
      if (seenNames.has(name)) {
        duplicateNames.push(name);
      } else {
        seenNames.add(name);
      }
    }

    if (duplicateNames.length > 0) {
      await message(`The following new file names are duplicated: ${duplicateNames.join(', ')}. Please adjust your regex or replacement text to ensure unique names.`, {
        title: 'Duplicate New Names',
        kind: 'error',
      });
      return;
    }

    await invoke("rename_files", {
      files: filesToRenamePayload,
    });

    if (currentDirectory.value) {
      try {
        const result = await invoke("read_files_in_directory", { path: currentDirectory.value });
        files.value = result as FileEntry[];
      } catch (readFilesError) {
        errorMessage.value = `Error updating file list: ${readFilesError}`;
        return;
      }
    } else {
      errorMessage.value = "No directory selected to update file list.";
      return;
    }

    errorMessage.value = "";
  } catch (error) {
    errorMessage.value = `Error renaming files: ${error}`;
  }
}

</script>

<template>
  <div class="container">
    <h1>Regex Renamer</h1>

    <button @click="_openDirectory">Select Folder</button>

    <div class="rename-controls">
      <input v-model="searchRegex" placeholder="Search Regex..." />
      <input v-model="replaceText" placeholder="Replace Text..." />
      <label>
        <input type="checkbox" v-model="preserveExtension" /> Preserve Extension
      </label>
      <button @click="_rename" :disabled="files.length === 0">Rename</button>
    </div>

    <p v-if="errorMessage" class="error-message">{{ errorMessage }}</p>

    <div class="file-list">
      <table>
        <thead>
          <tr>
            <th @click="_sortBy('name')">Original Name <span v-if="sortKey === 'name'">{{ sortOrder === 'asc' ? '▲' : '▼' }}</span></th>
            <th @click="_sortBy('newName')">New Name <span v-if="sortKey === 'newName'">{{ sortOrder === 'asc' ? '▲' : '▼' }}</span></th>
            <th @click="_sortBy('modified')">Last Modified <span v-if="sortKey === 'modified'">{{ sortOrder === 'asc' ? '▲' : '▼' }}</span></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="file in _sortedRenamedFiles" :key="file.path">
            <td>{{ file.name }}</td>
            <td :class="{ 'error': file.error }">{{ file.error || file.newName }}</td>
            <td>{{ new Date(file.modified).toLocaleString() }}</td>
          </tr>
        </tbody>
      </table>
    </div>

  </div>
</template>

<style scoped>
.container {
  padding: 2rem;
}

.rename-controls {
  display: flex;
  gap: 1rem;
  margin: 1rem 0;
}

.rename-controls input {
  flex-grow: 1;
}

.file-list table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
}

.file-list th, .file-list td {
  border: 1px solid #ddd;
  padding: 8px;
  text-align: left;
  overflow: hidden; /* Hide overflow content */
  text-overflow: ellipsis; /* Show ellipsis for overflow text */
  white-space: nowrap; /* Prevent text wrapping */
}

.file-list th {
  background-color: #f2f2f2;
}

.file-list th:nth-child(1) { width: 35%; } /* Original Name */
.file-list th:nth-child(2) { width: 35%; } /* New Name */
.file-list th:nth-child(3) { width: 30%; } /* Last Modified */

.error {
  color: red;
}

.error-message {
    color: red;
    margin-top: 1rem;
}
</style>