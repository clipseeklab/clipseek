export interface TagInputValue {
  names: string[];
  input: string;
}
export function collectTagNames(value: TagInputValue): string[] {
  return [
    ...new Set(
      [...value.names, ...value.input.split(/[,，\n]/)].map((name) => name.trim()).filter(Boolean),
    ),
  ];
}
