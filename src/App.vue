<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { message, open } from '@tauri-apps/plugin-dialog';
import { DataFrame, toJSON } from 'danfojs';
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
    return new DataFrame([]);
  }

  let df = new DataFrame(files.value);
  df.addColumn('newName', df['name'], { inplace: true });
  df.addColumn('error', new Array(df.index.length).fill(null), { inplace: true });

  if (!searchRegex.value && !replaceText.value) {
    return df;
  }
  
  try {
    const regex = searchRegex.value ? new RegExp(searchRegex.value, 'g') : null;

    const sequenceMatch = replaceText.value.match(/\{(\d+)\}/);
    const sequenceLength = sequenceMatch?.[1]?.length ?? 0;
    const startNumber = sequenceMatch?.[1] ? parseInt(sequenceMatch[1], 10) : 0;
    const sequencePlaceholder = sequenceMatch?.[0] ?? '';

    // Base newName calculation
    const newNames = df.apply((row) => {
      let baseName = row[0];
      if (typeof baseName !== 'string') {
        return baseName; // Return original value if not a string
      }
      let extension = '';
      if (preserveExtension.value) {
        const lastDotIndex = baseName.lastIndexOf('.');
        if (lastDotIndex > 0) { // a.txt -> .txt, .gitignore -> no extension
          extension = baseName.substring(lastDotIndex);
          baseName = baseName.substring(0, lastDotIndex);
        }
      }

      const replacedBase = regex ? baseName.replace(regex, replaceText.value) : replaceText.value;

      return replacedBase + extension;
    }, { axis: 1 });

    df.addColumn('newName', newNames.values, { inplace: true });

    // Apply sequence to sorted, affected files
    if (sequenceMatch) {
      // Create a temporary sorted DataFrame to determine sequence order
      const tempDf = df.sortValues(sortKey.value, { ascending: sortOrder.value === 'asc' });
      
      let sequenceCounter = 0;
      const finalNewNames = tempDf.apply((row) => {
        console.log("Processing row:", row);
        let currentNewName = row[3];
        if (row[0] !== currentNewName) { // Only apply sequence to changed names
          const paddedNumber = String(startNumber + sequenceCounter).padStart(sequenceLength, '0');
          if (typeof currentNewName === 'string') {
             currentNewName = currentNewName.replace(sequencePlaceholder, paddedNumber);
          }
          sequenceCounter++;
        }
        return currentNewName;
      }, { axis: 1 });


      // Update the original DataFrame based on the sorted one
      tempDf.addColumn('newName', finalNewNames.values, { inplace: true });
      // Re-sort by original index to align with the main 'df'
      df = tempDf.sortIndex();
    }
    
    return df;

  } catch (e) {
    const errorMessage = e instanceof Error ? e.message : String(e);
    let df = new DataFrame(files.value);
    df.addColumn('newName', df['name'], { inplace: true });
    df.addColumn('error', new Array(df.index.length).fill(errorMessage), { inplace: true });
    return df;
  }
});

const sortedRenamedFiles = computed(() => {
  const df = processedFiles.value;
  if (!df || df.shape[0] === 0) {
    return [];
  }
  
  const sortedDf = df.sortValues(sortKey.value, { ascending: sortOrder.value === 'asc' });
  return toJSON(sortedDf);
});

async function _rename() {
  try {
    const df = processedFiles.value;
    if (df.shape[0] === 0) return;

    const filesToRenameJson = toJSON(df.query(df['name'].ne(df['newName']))) as FileEntry[];
    const filesToRenamePayload = filesToRenameJson.map(f => ({
      name: f.name,
      path: f.path,
      modified: f.modified,
      new_name: f.newName
    }));

    if (filesToRenamePayload.length === 0) return;

    const allFilesJson = toJSON(df) as FileEntry[];

    // Check for empty new names
    const emptyNames = allFilesJson.filter(f => f.newName === '');
    if (emptyNames.length > 0) {
      await message('One or more files would be renamed to an empty name. Please adjust your regex or replacement text.', {
        title: 'Invalid Rename',
        kind: 'error',
      });
      return;
    }

    const extensionOnlyNames = allFilesJson.filter(f => {
      // Check if the new name is just an extension (e.g., ".txt") or just "."
      return f.newName.startsWith('.') && f.newName.length > 1 && f.newName.substring(1).indexOf('.') === -1 && f.newName.substring(1).length > 0;
    });

    if (extensionOnlyNames.length > 0) {
      await message('One or more files would be renamed to an extension only (e.g., ".txt"). Please adjust your regex or replacement text.', {
        title: 'Invalid Rename',
        kind: 'error',
      });
      return;
    }

    // Check for duplicate new names
    const newNames = df['newName'].values;
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
        console.error("Error re-reading files after rename:", readFilesError);
        errorMessage.value = `Error updating file list: ${readFilesError}`;
        return; // Stop execution if file list update fails
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
      <button @click="_rename" :disabled="!processedFiles.value || processedFiles.value.shape[0] === 0 || processedFiles.value.column('error').count() > 0">Rename</button>
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
          <tr v-for="file in sortedRenamedFiles" :key="file.path">
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