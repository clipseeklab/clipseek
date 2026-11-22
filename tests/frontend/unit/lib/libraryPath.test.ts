import { describe, expect, it } from "vitest";
import {
  libraryDirectoryName,
  libraryPath,
  selectedDirectoryName,
  libraryNameError,
} from "@/lib/libraryPath";

describe("library paths", () => {
  it("takes the last segment from Windows and POSIX selections", () => {
    expect(selectedDirectoryName("C:\\Users\\Alice\\Pictures\\")).toBe("Pictures");
    expect(selectedDirectoryName("/Users/Alice/Pictures/")).toBe("Pictures");
  });

  it("avoids Windows reserved directory names and trailing dots", () => {
    expect(libraryDirectoryName("CON")).toBe("CON-Library");
    expect(libraryDirectoryName("com1.txt")).toBe("com1-Library.txt");
    expect(libraryDirectoryName("Design. ")).toBe("Design");
    expect(libraryPath("C:\\Users\\Alice\\", "My:Assets")).toBe("C:\\Users\\Alice\\My-Assets");
  });
});

describe("portable library name validation", () => {
  it.each([
    "旅行素材",
    "工作 灵感",
    "CONtent",
    "com10",
    "COM1片段",
    "旅行：日落",
    "中".repeat(85),
    "a".repeat(255),
  ])("accepts supported names: %s", (name) => {
    expect(libraryNameError(name)).toBeNull();
  });
  it.each(["<", ">", ":", '"', "/", "\\", "|", "?", "*", "\x00", "\x1f", "\x7f", "\x85"])(
    "rejects unsupported characters: %s",
    (char) => {
      expect(libraryNameError(`My${char}Library`)).toBe("characters");
    },
  );
  it.each(["CON", "con.jpg", "PRN", "AUX", "NUL", "COM9", "com¹.txt", "LPT²", "CONOUT$"])(
    "rejects reserved names: %s",
    (name) => {
      expect(libraryNameError(name)).toBe("reserved");
    },
  );
  it("checks empty names, trailing spaces/dots and UTF-8 size", () => {
    expect(libraryNameError(" ")).toBe("empty");
    expect(libraryNameError("name.")).toBe("trailing");
    expect(libraryNameError("name ")).toBe("trailing");
    expect(libraryNameError("中".repeat(86))).toBe("tooLong");
    expect(libraryNameError("a".repeat(256))).toBe("tooLong");
  });
});
