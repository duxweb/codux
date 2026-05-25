export function displayPath(value?: string | null) {
  const path = value?.trim();
  if (!path) return "";
  return path.replace(/^\\\\\?\\UNC\\/i, "\\\\").replace(/^\\\\\?\\/i, "").replace(/^\/\/\?\//, "");
}

export function displayPathBasename(value?: string | null) {
  const path = displayPath(value).replace(/\\/g, "/").replace(/\/+$/, "");
  if (!path) return "";
  return path.split("/").pop() || path;
}
