/** Keep a library directory name portable between macOS and Windows. */
export function libraryDirectoryName(name: string): string {
  let value = Array.from(name.trim(), (char) =>
    char.charCodeAt(0) <= 0x1f || /[/\\:*?"<>|]/.test(char) ? "-" : char,
  )
    .join("")
    .replace(/[. ]+$/, "");
  if (!value) return "Library";
  const stem = value.split(".", 1)[0];
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i.test(stem)) {
    value = value.replace(/^([^.]*)/, "$1-Library");
  }
  return value;
}
export function selectedDirectoryName(path: string): string {
  return (
    path
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() ?? ""
  );
}
export function libraryPath(parent: string, name: string): string {
  if (!parent) return "";
  const separator = parent.includes("\\") ? "\\" : "/";
  return `${parent.replace(/[\\/]+$/, "")}${separator}${libraryDirectoryName(name)}`;
}
export type LibraryNameError = "empty" | "characters" | "reserved" | "trailing" | "tooLong";
/** Validate the name before deriving a directory, rather than silently renaming it. */
export function hasInvalidLibraryNameCharacters(name: string): boolean {
  return Array.from(name).some((char) => {
    const code = char.charCodeAt(0);
    return code <= 0x1f || (code >= 0x7f && code <= 0x9f) || /[/\\:*?"<>|]/.test(char);
  });
}
export function libraryNameError(name: string): LibraryNameError | null {
  if (!name.trim()) return "empty";
  if (hasInvalidLibraryNameCharacters(name)) return "characters";
  if (/[. ]$/.test(name)) return "trailing";
  const stem = name.trimStart().split(".", 1)[0].trimEnd();
  if (/^(con|prn|aux|nul|conin\$|conout\$|com[1-9¹²³]|lpt[1-9¹²³])$/i.test(stem)) return "reserved";
  // A conservative portable component limit; includes multibyte Chinese/emoji.
  if (new TextEncoder().encode(name.trim()).length > 255) return "tooLong";
  return null;
}
