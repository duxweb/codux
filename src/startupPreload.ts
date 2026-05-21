import { invoke } from "@tauri-apps/api/core";
import type { AIGlobalHistorySnapshot } from "./ai/history";
import type { PetSnapshot } from "./ai/petState";
import type { AIRuntimeStateSnapshot } from "./ai/types";
import { readCachedProjectListSnapshot, writeCachedProjectListSnapshot } from "./projectSnapshotCache";
import { useRuntimeStore } from "./runtimeStore";
import type { ProjectListSnapshot, RemoteStatus } from "./types";

let startupPreloadPromise: Promise<void> | null = null;

export function preloadRuntimeSnapshots() {
  if (!window.__TAURI_INTERNALS__) return Promise.resolve();
  if (startupPreloadPromise) return startupPreloadPromise;
  startupPreloadPromise = preloadRuntimeSnapshotsOnce().finally(() => {
    startupPreloadPromise = null;
  });
  return startupPreloadPromise;
}

async function preloadRuntimeSnapshotsOnce() {
  const store = useRuntimeStore.getState();
  const cachedProjectList = store.projectListSnapshot ?? readCachedProjectListSnapshot();
  if (cachedProjectList) {
    store.setProjectListSnapshot(cachedProjectList);
  }

  const projectListPromise = invoke<ProjectListSnapshot>("project_list")
    .then((snapshot) => {
      store.setProjectListSnapshot(snapshot);
      writeCachedProjectListSnapshot(snapshot);
      return snapshot;
    })
    .catch((error) => {
      console.error("failed to preload project list", error);
      return cachedProjectList;
    });

  const preloadTasks: Array<Promise<unknown>> = [];

  preloadTasks.push(
    invoke<RemoteStatus>("remote_status")
      .then((status) => useRuntimeStore.getState().setRemoteStatus(status))
      .catch((error) => console.error("failed to preload remote status", error)),
  );

  preloadTasks.push(
    invoke<AIRuntimeStateSnapshot>("ai_runtime_state_snapshot")
      .then((snapshot) => useRuntimeStore.getState().setAIRuntimeSnapshot(snapshot))
      .catch((error) => console.error("failed to preload ai runtime state", error)),
  );

  preloadTasks.push(
    invoke<PetSnapshot>("pet_snapshot")
      .then((snapshot) => useRuntimeStore.getState().setPetSnapshot(snapshot))
      .catch((error) => console.error("failed to preload pet snapshot", error)),
  );

  const projectList = await projectListPromise;
  if (!projectList?.projects.length) {
    await Promise.allSettled(preloadTasks);
    return;
  }

  const projects = projectList.projects.map((project) => ({
    id: project.id,
    name: project.name,
    path: project.path,
  }));
  preloadTasks.push(
    invoke<AIGlobalHistorySnapshot>("ai_history_global_state", { projects })
      .then((snapshot) => {
        if (snapshot) useRuntimeStore.getState().setAIGlobalHistory(snapshot);
      })
      .catch((error) => console.error("failed to preload ai history state", error)),
  );
  await Promise.allSettled(preloadTasks);
}
