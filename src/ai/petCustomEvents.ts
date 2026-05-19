import { emit, listen } from "@tauri-apps/api/event";
import type { PetCustomPet } from "./petState";

export const PET_CUSTOM_PET_INSTALLED_EVENT = "codux:pet-custom-installed";

export async function emitPetCustomPetInstalled(pet: PetCustomPet) {
  if (window.__TAURI_INTERNALS__) {
    await emit(PET_CUSTOM_PET_INSTALLED_EVENT, pet);
    return;
  }
  window.dispatchEvent(new CustomEvent(PET_CUSTOM_PET_INSTALLED_EVENT, { detail: pet }));
}

export async function listenPetCustomPetInstalled(handler: (pet: PetCustomPet) => void) {
  if (window.__TAURI_INTERNALS__) {
    return listen<PetCustomPet>(PET_CUSTOM_PET_INSTALLED_EVENT, (event) => handler(event.payload));
  }
  const listener = (event: Event) => {
    handler((event as CustomEvent<PetCustomPet>).detail);
  };
  window.addEventListener(PET_CUSTOM_PET_INSTALLED_EVENT, listener);
  return () => window.removeEventListener(PET_CUSTOM_PET_INSTALLED_EVENT, listener);
}
