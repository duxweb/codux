import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import { resolve } from "node:path";

function packageNameFromModuleId(id: string) {
  const marker = "/node_modules/";
  const index = id.lastIndexOf(marker);
  if (index === -1) return "";
  const parts = id.slice(index + marker.length).split("/");
  if (parts[0]?.startsWith("@")) return `${parts[0]}/${parts[1] ?? ""}`;
  return parts[0] ?? "";
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  assetsInclude: ["**/*.wasm"],
  server: {
    strictPort: true,
    port: 1420,
    host: "127.0.0.1",
    watch: {
      ignored: ["**/src-tauri/target/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        desktopPet: resolve(__dirname, "desktop-pet.html"),
      },
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return;
          const packageName = packageNameFromModuleId(id);
          if (packageName === "@xterm/xterm") return "vendor-xterm-core";
          if (packageName === "@xterm/addon-webgl") return "vendor-xterm-webgl";
          if (packageName.startsWith("@xterm/")) return "vendor-xterm-addons";
          if (packageName === "@codemirror/view" || packageName === "@codemirror/state") return "vendor-editor-core";
          if (packageName === "codemirror") return "vendor-editor-setup";
          if (
            packageName === "@codemirror/language" ||
            packageName === "@codemirror/commands" ||
            packageName.startsWith("@lezer/")
          ) {
            return "vendor-editor-language";
          }
          if (packageName === "@codemirror/search") return "vendor-editor-search";
          if (packageName.startsWith("@codemirror/lang-")) return "vendor-editor-langs";
          if (packageName === "@codemirror/legacy-modes") return "vendor-editor-legacy";
          if (
            packageName.startsWith("@heroui/") ||
            packageName.startsWith("@floating-ui/") ||
            packageName.startsWith("react-aria") ||
            packageName.startsWith("@react-aria/") ||
            packageName.startsWith("@react-stately/")
          ) {
            return "vendor-ui";
          }
          if (
            packageName === "react" ||
            packageName === "react-dom" ||
            packageName === "scheduler" ||
            packageName === "use-sync-external-store"
          ) {
            return "vendor-react";
          }
          if (packageName.startsWith("@heroicons/")) return "vendor-icons";
        },
      },
    },
  },
});
