import { invoke } from "@tauri-apps/api/core";

export type ProjectOpenApplication = {
  id: string;
  label: string;
  category: "primary" | "ide";
  installed: boolean;
};

export async function listProjectOpenApplications() {
  if (!window.__TAURI_INTERNALS__) return previewApplications();
  return invoke<ProjectOpenApplication[]>("project_open_applications");
}

export async function openProjectInApplication(projectPath: string, applicationId: string) {
  if (!window.__TAURI_INTERNALS__) return;
  return invoke<void>("project_open_in_application", {
    request: {
      projectPath,
      applicationId,
    },
  });
}

export async function revealProjectInFileManager(projectPath: string) {
  if (!window.__TAURI_INTERNALS__) return;
  return invoke<void>("project_reveal_in_file_manager", { projectPath });
}

function previewApplications(): ProjectOpenApplication[] {
  return [
    { id: "vscode", label: "VS Code", category: "primary", installed: true },
    { id: "terminal", label: "Terminal", category: "primary", installed: true },
    { id: "cursor", label: "Cursor", category: "ide", installed: true },
  ];
}
