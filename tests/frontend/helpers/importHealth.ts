/** Browser-side import diagnostics; functions passed to execute must be self-contained. */
export interface ImportHealthEvent {
  kind: string;
  message: string;
  at: number;
  alt?: string;
  stack?: string;
}
export interface ImportHealthReport {
  events: ImportHealthEvent[];
  images: { name: string; decoded: boolean }[];
  overflow: boolean;
}
export interface ImportHealthOptions {
  expectedImages: string[];
  expectedBrokenImages?: string[];
}
interface Monitor {
  read(stop: boolean): ImportHealthReport;
}
declare global {
  interface Window {
    __clipseekImportHealth?: Monitor;
  }
}

export function installImportHealthMonitor(options: ImportHealthOptions): void {
  if (window.__clipseekImportHealth) throw new Error("Import health monitor already installed");
  const events: ImportHealthEvent[] = [];
  const imageRecovery = new Map<string, boolean>();
  let overflow = false;
  const seen = new WeakMap<Element, string>();
  const record = (kind: string, message: string, alt?: string, stack?: string) => {
    if (events.length >= 100) {
      overflow = true;
      return;
    }
    events.push({
      kind,
      message: message.slice(0, 1000),
      at: Date.now(),
      ...(alt === undefined ? {} : { alt }),
      ...(stack ? { stack: stack.slice(0, 4000) } : {}),
    });
  };
  // Toaster has no alert role yet. Keep its structural fallback here and exercise
  // it against the real component, so CSS/markup changes cannot silently disable it.
  const selector = '[role="alert"], div.pointer-events-auto, button.pointer-events-auto';
  const inspect = (root: Node) => {
    if (!(root instanceof Element)) return;
    const nodes = [...(root.matches(selector) ? [root] : []), ...root.querySelectorAll(selector)];
    for (const node of nodes) {
      if (node.getAttribute("role") !== "alert" && !node.querySelector(":scope > svg.text-red-600"))
        continue;
      const message = node.textContent?.trim() ?? "";
      if (message && seen.get(node) !== message) {
        seen.set(node, message);
        record("toast", message);
      }
    }
  };
  const mutations = (records: MutationRecord[]) => {
    for (const change of records) {
      for (const node of [...change.addedNodes, ...change.removedNodes]) inspect(node);
      if (change.type === "characterData" && change.target.parentElement)
        inspect(change.target.parentElement);
    }
  };
  const observer = new MutationObserver(mutations);
  observer.observe(document.body, { childList: true, subtree: true, characterData: true });
  inspect(document.body);
  const describe = (value: unknown): string => {
    if (value instanceof Error) return value.stack ?? value.message;
    try {
      return typeof value === "string" ? value : (JSON.stringify(value) ?? String(value));
    } catch {
      return String(value);
    }
  };
  const onError = (event: Event) => {
    if (event.target instanceof HTMLImageElement) {
      const image = event.target;
      if (!options.expectedBrokenImages?.includes(image.alt)) {
        imageRecovery.set(image.alt, false);
        record("image-error", image.currentSrc || image.src, image.alt);
      }
    } else if (event instanceof ErrorEvent) record("window-error", event.message);
  };
  const onLoad = (event: Event) => {
    const image = event.target;
    if (
      image instanceof HTMLImageElement &&
      imageRecovery.has(image.alt) &&
      image.complete &&
      image.naturalWidth > 0 &&
      image.naturalHeight > 0
    )
      imageRecovery.set(image.alt, true);
  };
  const onRejection = (event: PromiseRejectionEvent) =>
    record("unhandled-rejection", describe(event.reason));
  const originalError = console.error;
  const loggedError = (...args: unknown[]) => {
    record(
      "console-error",
      args.map(describe).join(" "),
      undefined,
      new Error("console.error call site").stack,
    );
    originalError.apply(console, args);
  };
  console.error = loggedError;
  window.addEventListener("error", onError, true);
  document.addEventListener("load", onLoad, true);
  window.addEventListener("unhandledrejection", onRejection);
  window.__clipseekImportHealth = {
    read(stop) {
      mutations(observer.takeRecords());
      inspect(document.body);
      const images = options.expectedImages.map((name) => ({
        name,
        decoded: [...document.querySelectorAll("img")].some(
          (image) =>
            image.alt === name &&
            image.complete &&
            image.naturalWidth > 0 &&
            image.naturalHeight > 0,
        ),
      }));
      // Virtual cards may decode a fallback and unmount before the final snapshot.
      // Preserve that observed recovery; unmounted failures remain unresolved.
      for (const [name, decoded] of imageRecovery) {
        if (decoded && !options.expectedImages.includes(name)) images.push({ name, decoded });
      }
      if (stop) {
        observer.disconnect();
        window.removeEventListener("error", onError, true);
        document.removeEventListener("load", onLoad, true);
        window.removeEventListener("unhandledrejection", onRejection);
        if (console.error === loggedError) console.error = originalError;
        delete window.__clipseekImportHealth;
      }
      return { events: [...events], images, overflow };
    },
  };
}

export function readImportHealthMonitor(stop: boolean): ImportHealthReport {
  if (!window.__clipseekImportHealth)
    throw new Error("Import health monitor lost; renderer may have reloaded");
  return window.__clipseekImportHealth.read(stop);
}

export function importHealthProblems(report: ImportHealthReport): string[] {
  const errors = report.events
    .filter((event) => event.kind !== "image-error")
    .map((event) => `${event.kind}: ${event.message}`);
  for (const image of report.images)
    if (!image.decoded) errors.push(`Image missing or undecodable: ${image.name}`);
  // Recovered image requests stay in the diagnostic report. A functioning IPC
  // fallback is supported; unresolved images and every global error still fail.
  for (const event of report.events.filter((event) => event.kind === "image-error")) {
    if (!report.images.some((image) => image.name === event.alt && image.decoded))
      errors.push(`Unrecovered image error: ${event.alt || event.message}`);
  }
  if (report.overflow) errors.push("Import diagnostic event limit exceeded");
  return errors;
}
