import { act, fireEvent, screen, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster, toast, useToastStore } from "@/components/Toaster";
import { renderWithProviders } from "@test/helpers/render";
import {
  installImportHealthMonitor,
  readImportHealthMonitor,
  importHealthProblems,
} from "@test/helpers/importHealth";

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  if (window.__clipseekImportHealth) readImportHealthMonitor(true);
  useToastStore.setState({ toasts: [] });
  cleanup();
  document.querySelectorAll("img").forEach((node) => node.remove());
  vi.restoreAllMocks();
  vi.clearAllTimers();
  vi.useRealTimers();
});

function image(name: string, decoded = true) {
  const node = document.createElement("img");
  node.alt = name;
  Object.defineProperties(node, {
    complete: { value: true },
    naturalWidth: { value: decoded ? 32 : 0 },
    naturalHeight: { value: decoded ? 32 : 0 },
  });
  document.body.append(node);
  return node;
}

describe("continuous import health monitor", () => {
  it("can be installed as a serialized browser function without module closures", () => {
    const remoteInstall = new Function(
      `return (${installImportHealthMonitor.toString()})`,
    )() as typeof installImportHealthMonitor;
    remoteInstall({ expectedImages: [] });
    expect(importHealthProblems(readImportHealthMonitor(true))).toEqual([]);
  });
  it("remembers a real error toast even after it disappears and the image is healthy", async () => {
    renderWithProviders(<Toaster />);
    installImportHealthMonitor({ expectedImages: ["ok.png"] });
    image("ok.png");
    act(() => toast.error("IO error: file not found"));
    expect(screen.getByText("IO error: file not found")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(3500);
    });
    await Promise.resolve();
    expect(screen.queryByText("IO error: file not found")).toBeNull();
    const report = readImportHealthMonitor(true);
    expect(report.images[0].decoded).toBe(true);
    expect(importHealthProblems(report)).toContain("toast: IO error: file not found");
  });
  it("allows real success notifications without hiding later errors", async () => {
    renderWithProviders(<Toaster />);
    installImportHealthMonitor({ expectedImages: [] });
    act(() => toast.success("Imported"));
    await Promise.resolve();
    expect(importHealthProblems(readImportHealthMonitor(false))).toEqual([]);
    act(() => toast.error("Unexpected failure"));
    await Promise.resolve();
    expect(importHealthProblems(readImportHealthMonitor(true))).toEqual([
      "toast: Unexpected failure",
    ]);
  });
  it("captures script errors, unhandled promises and console errors until stopped", () => {
    const original = vi.spyOn(console, "error").mockImplementation(() => {});
    installImportHealthMonitor({ expectedImages: [] });
    window.dispatchEvent(new ErrorEvent("error", { message: "Script crashed" }));
    const rejection = new Event("unhandledrejection");
    Object.defineProperty(rejection, "reason", { value: new Error("Promise failed") });
    window.dispatchEvent(rejection);
    console.error("Render failed");
    const report = readImportHealthMonitor(true);
    expect(report.events.map((event) => event.kind)).toEqual([
      "window-error",
      "unhandled-rejection",
      "console-error",
    ]);
    expect(report.events[2].stack).toBeTruthy();
    expect(console.error).toBe(original);
    window.dispatchEvent(new ErrorEvent("error", { message: "after stop" }));
    expect(report.events).toHaveLength(3);
  });
  it("fails for missing and undecodable imported images", () => {
    installImportHealthMonitor({ expectedImages: ["missing.png", "bad.png"] });
    const broken = image("bad.png", false);
    fireEvent.error(broken);
    const problems = importHealthProblems(readImportHealthMonitor(true));
    expect(problems).toContain("Image missing or undecodable: missing.png");
    expect(problems).toContain("Image missing or undecodable: bad.png");
    expect(problems).toContain("Unrecovered image error: bad.png");
  });
  it("records recovered image errors while allowing a successful fallback", () => {
    installImportHealthMonitor({ expectedImages: ["ok.png"] });
    const failed = image("ok.png", false);
    fireEvent.error(failed);
    failed.remove();
    image("ok.png");
    const report = readImportHealthMonitor(true);
    expect(report.events).toHaveLength(1);
    expect(importHealthProblems(report)).toEqual([]);
  });
  it("remembers a decoded fallback after a virtual card leaves the viewport", () => {
    installImportHealthMonitor({ expectedImages: [] });
    const failed = image("virtual.png", false);
    fireEvent.error(failed);
    failed.remove();
    const fallback = image("virtual.png");
    fireEvent.load(fallback);
    fallback.remove();
    const report = readImportHealthMonitor(true);
    expect(report.events).toHaveLength(1);
    expect(importHealthProblems(report)).toEqual([]);
  });
  it("still rejects an unmounted failure and a failure after an earlier decode", () => {
    installImportHealthMonitor({ expectedImages: [] });
    const healthy = image("virtual.png");
    fireEvent.load(healthy);
    healthy.remove();
    const failed = image("virtual.png", false);
    fireEvent.error(failed);
    failed.remove();
    expect(importHealthProblems(readImportHealthMonitor(true))).toEqual([
      "Unrecovered image error: virtual.png",
    ]);
  });
  it("limits expected image failures to named fixtures and still rejects global errors", () => {
    installImportHealthMonitor({ expectedImages: [], expectedBrokenImages: ["intentional.png"] });
    fireEvent.error(image("intentional.png", false));
    fireEvent.error(image("unexpected.png", false));
    window.dispatchEvent(new ErrorEvent("error", { message: "unexpected runtime error" }));
    const report = readImportHealthMonitor(true);
    expect(report.events.map((event) => event.alt).filter(Boolean)).toEqual(["unexpected.png"]);
    expect(importHealthProblems(report)).toContain("window-error: unexpected runtime error");
  });
  it("fails on diagnostic overflow instead of silently dropping a flood", () => {
    installImportHealthMonitor({ expectedImages: [] });
    for (let n = 0; n < 101; n++)
      window.dispatchEvent(new ErrorEvent("error", { message: `failure ${n}` }));
    const report = readImportHealthMonitor(true);
    expect(report.events).toHaveLength(100);
    expect(importHealthProblems(report)).toContain("Import diagnostic event limit exceeded");
  });
  it("rejects a missing monitor instead of accepting a renderer reload as healthy", () => {
    expect(() => readImportHealthMonitor(true)).toThrow("Import health monitor lost");
  });
});
