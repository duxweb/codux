import { invoke } from "@tauri-apps/api/core";

export interface FileEntry {
  path: string;
  relativePath: string;
  name: string;
  isDirectory: boolean;
  isSymbolicLink: boolean;
  size: number;
  modifiedAt: number;
}

export interface FileReadResult {
  path: string;
  relativePath: string;
  name: string;
  content: string;
  size: number;
  modifiedAt: number;
  isBinary: boolean;
  isLarge: boolean;
  isTruncated: boolean;
  readOnly: boolean;
  message?: string | null;
}

export interface FileChangeEvent {
  projectPath: string;
  changedPaths: string[];
}

export interface FileWatchRegistration {
  projectPath: string;
}

export async function listFileChildren(rootPath: string, directoryPath?: string) {
  if (!window.__TAURI_INTERNALS__) return previewEntries(rootPath, directoryPath);
  return invoke<FileEntry[]>("file_list_children", {
    request: {
      rootPath,
      directoryPath,
    },
  });
}

export async function readFile(rootPath: string, path: string) {
  if (!window.__TAURI_INTERNALS__) {
    return {
      path,
      relativePath: path.split("/").pop() || path,
      name: path.split("/").pop() || path,
      content: "Preview mode uses the native app to read project files.\n",
      size: 0,
      modifiedAt: 0,
      isBinary: false,
      isLarge: false,
      isTruncated: false,
      readOnly: true,
      message: null,
    } satisfies FileReadResult;
  }
  return invoke<FileReadResult>("file_read", {
    request: {
      rootPath,
      path,
    },
  });
}

export async function writeFile(rootPath: string, path: string, content: string) {
  return invoke<FileReadResult>("file_write", {
    request: {
      rootPath,
      path,
      content,
    },
  });
}

export async function createFile(rootPath: string, parentPath: string | undefined, name: string) {
  return invoke<FileEntry>("file_create_file", {
    request: {
      rootPath,
      parentPath,
      name,
    },
  });
}

export async function createDirectory(rootPath: string, parentPath: string | undefined, name: string) {
  return invoke<FileEntry>("file_create_dir", {
    request: {
      rootPath,
      parentPath,
      name,
    },
  });
}

export async function renameFile(rootPath: string, path: string, newName: string) {
  return invoke<FileEntry>("file_rename", {
    request: {
      rootPath,
      path,
      newName,
    },
  });
}

export async function deleteFile(rootPath: string, path: string) {
  return invoke<void>("file_delete", {
    request: {
      rootPath,
      path,
    },
  });
}

export async function copyFile(rootPath: string, sourcePath: string, targetDirectoryPath?: string) {
  return invoke<FileEntry>("file_copy", {
    request: {
      rootPath,
      sourcePath,
      targetDirectoryPath,
    },
  });
}

export async function importExternalFiles(rootPath: string, sourcePaths: string[], targetDirectoryPath?: string) {
  return invoke<FileEntry[]>("file_import_external", {
    request: {
      rootPath,
      sourcePaths,
      targetDirectoryPath,
    },
  });
}

export async function revealFile(rootPath: string, path: string) {
  return invoke<void>("file_reveal", {
    request: {
      rootPath,
      path,
    },
  });
}

export async function openFileExternally(rootPath: string, path: string) {
  return invoke<void>("file_open", {
    request: {
      rootPath,
      path,
    },
  });
}

export async function watchProjectFiles(projectPath: string) {
  return invoke<FileWatchRegistration>("file_watch", { projectPath });
}

export async function unwatchProjectFiles(projectPath: string) {
  return invoke<void>("file_unwatch", { projectPath });
}

export function languageForPath(path: string) {
  const name = path.split("/").pop()?.toLowerCase() ?? path.toLowerCase();
  const ext = name.includes(".") ? name.split(".").pop() ?? "" : "";
  if (["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "vue", "svelte"].includes(ext)) return "javascript";
  if (["json", "json5", "jsonl", "map"].includes(ext) || name.endsWith(".jsonc")) return "json";
  if (["css", "scss", "sass", "less"].includes(ext)) return "css";
  if (["html", "htm", "xhtml"].includes(ext)) return "html";
  if (["md", "markdown", "mdx"].includes(ext)) return "markdown";
  if (["py", "pyw"].includes(ext)) return "python";
  if (ext === "rs") return "rust";
  if (ext === "go") return "go";
  if (["xml", "svg", "plist", "xaml"].includes(ext)) return "xml";
  if (["sql", "psql", "mysql"].includes(ext)) return "sql";
  if (["yml", "yaml"].includes(ext)) return "yaml";
  if (ext === "toml") return "toml";
  if (["ini", "conf", "config", "env", "lock", "gitignore", "dockerignore", "editorconfig"].includes(ext) || name.startsWith(".env")) return "properties";
  if (name === "dockerfile" || name.endsWith(".dockerfile")) return "dockerfile";
  if (["diff", "patch"].includes(ext)) return "diff";
  if (["sh", "bash", "zsh", "fish", "ps1", "bat", "cmd"].includes(ext)) return "shell";
  if (ext === "rb") return "ruby";
  if (ext === "java") return "java";
  if (["kt", "kts"].includes(ext)) return "kotlin";
  if (ext === "swift") return "swift";
  if (["c", "h"].includes(ext)) return "c";
  if (["cpp", "cc", "cxx", "hpp"].includes(ext)) return "cpp";
  if (ext === "cs") return "csharp";
  if (ext === "dart") return "dart";
  if (ext === "lua") return "lua";
  if (ext === "r") return "r";
  if (ext === "php") return "html";
  return "plain";
}

function previewEntries(rootPath: string, directoryPath?: string): FileEntry[] {
  const root = rootPath || "/Volumes/Web/codux-tauri";
  if (directoryPath && directoryPath !== root) return [];
  return [
    previewEntry(root, "src", true),
    previewEntry(root, "src-tauri", true),
    previewEntry(root, "package.json", false),
    previewEntry(root, "README.md", false),
  ];
}

function previewEntry(root: string, name: string, isDirectory: boolean): FileEntry {
  return {
    path: `${root.replace(/\/$/, "")}/${name}`,
    relativePath: name,
    name,
    isDirectory,
    isSymbolicLink: false,
    size: 0,
    modifiedAt: 0,
  };
}
