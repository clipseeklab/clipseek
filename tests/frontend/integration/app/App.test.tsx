import { act, screen, waitFor } from "@test/helpers/render";
import { renderWithProviders } from "@test/helpers/render";
import userEvent from "@testing-library/user-event";
import { useAppStore } from "@/stores/app";
import { queryKeys } from "@/lib/queryKeys";
import type { LibraryMeta, OnboardingState, Settings } from "@/types";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  modelStatus: vi.fn(),
  libraryList: vi.fn(),
  libraryCurrent: vi.fn(),
  librarySwitch: vi.fn(),
  libraryReauthorize: vi.fn(),
  libraryDelete: vi.fn(),
  libraryImport: vi.fn(),
  libraryCreate: vi.fn(),
  openDialog: vi.fn(),
  settingsGet: vi.fn(),
  onboardingGet: vi.fn(),
  onboardingSet: vi.fn(),
  applyTheme: vi.fn(),
  detectSystemLanguage: vi.fn(() => "en"),
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  unlisten: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    modelStatus: mocks.modelStatus,
    libraryList: mocks.libraryList,
    libraryCurrent: mocks.libraryCurrent,
    librarySwitch: mocks.librarySwitch,
    libraryReauthorize: mocks.libraryReauthorize,
    libraryDelete: mocks.libraryDelete,
    libraryImport: mocks.libraryImport,
    libraryCreate: mocks.libraryCreate,
    settingsGet: mocks.settingsGet,
    onboardingGet: mocks.onboardingGet,
    onboardingSet: mocks.onboardingSet,
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openDialog }));
vi.mock("@tauri-apps/api/path", () => ({ homeDir: () => Promise.resolve("/home/test") }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, callback: (event: { payload: unknown }) => void) => {
    mocks.listeners.set(event, callback);
    return Promise.resolve(mocks.unlisten);
  }),
}));

vi.mock("@/lib/theme", () => ({ applyTheme: mocks.applyTheme }));
vi.mock("@/i18n", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/i18n")>();
  return { ...original, detectSystemLanguage: mocks.detectSystemLanguage };
});

vi.mock("@/components/layout/AppLayout", () => ({
  default: () => <div data-testid="app-layout" />,
}));
vi.mock("@/components/ModelDownloadGate", () => ({
  default: () => <div data-testid="model-download-gate" />,
}));
vi.mock("@/components/wizard/FirstRunWizard", () => ({
  default: ({ initialState }: { initialState: OnboardingState }) => (
    <div data-testid="first-run-wizard" data-step={initialState.currentStep} />
  ),
}));

import App from "@/App";
import { loadTipsState } from "@/lib/usageTips";

const library: LibraryMeta = {
  id: "library-1",
  name: "Main",
  path: "/library/main",
  createdAt: 1,
};

const settings: Settings = {
  language: "en",
  theme: "light",
  trashRetentionDays: 30,

  importMode: "reference",
};

const completedOnboarding: OnboardingState = {
  version: 1,
  currentStep: 3,
  completed: true,
};

function resetAppStore() {
  useAppStore.setState({
    libraries: [],
    current: null,
    settings: {
      ...settings,
      language: "auto",
      theme: "auto",
    },
    loaded: false,
    sidebarHidden: false,
    gridColumns: 4,
    recentTagNames: [],
    volumeWarning: null,
  });
}

function unavailableLibrary(opening?: Promise<void>) {
  const replacement: LibraryMeta = {
    id: "library-2",
    name: "Archive",
    path: "/library/archive",
    createdAt: 2,
  };
  let active = library;
  let registered = [library, replacement];
  mocks.libraryCurrent.mockImplementation(async () => active);
  mocks.libraryList.mockImplementation(async () => registered);
  mocks.librarySwitch.mockImplementation(async (id: string) => {
    if (id === library.id) throw { code: "library_dir_invalid", message: "missing directory" };
    if (opening) await opening;
    active = replacement;
    return replacement;
  });
  mocks.libraryDelete.mockImplementation(async (id: string, deleteFiles: boolean) => {
    expect(active.id).toBe(replacement.id);
    expect(deleteFiles).toBe(false);
    registered = registered.filter((item) => item.id !== id);
  });
  mocks.libraryImport.mockImplementation(async () => {
    active = replacement;
    return replacement;
  });
  mocks.libraryCreate.mockImplementation(async () => {
    active = replacement;
    return replacement;
  });
  return replacement;
}

describe("App startup integration", () => {
  it("arms first-use tips while the initial setup is still incomplete", async () => {
    mocks.onboardingGet.mockResolvedValue({ version: 2, currentStep: 3, completed: false });
    renderWithProviders(<App />);
    await screen.findByTestId("first-run-wizard");
    expect(loadTipsState().pending).toBe(true);
    expect(loadTipsState().presented).toBe(false);
  });

  it("starts tips from the first card when onboarding is repeated with old reading history", async () => {
    localStorage.setItem(
      "clipstudio-open:usage-tips",
      JSON.stringify({
        pending: false,
        presented: true,
        lastId: "clip",
        seen: { clip: 1 },
        announced: { clip: 1 },
      }),
    );
    mocks.onboardingGet.mockResolvedValue({ version: 2, currentStep: 3, completed: false });
    renderWithProviders(<App />);
    await screen.findByTestId("first-run-wizard");
    expect(loadTipsState()).toMatchObject({ pending: true, presented: false, lastId: "visual" });
  });

  it("restores first-use tips suppressed before they were ever presented", async () => {
    localStorage.setItem(
      "clipstudio-open:usage-tips",
      JSON.stringify({
        pending: false,
        presented: false,
        lastId: "visual",
        seen: {},
        announced: {},
      }),
    );
    renderWithProviders(<App />);
    await screen.findByTestId("app-layout");
    expect(loadTipsState().pending).toBe(true);
  });

  it.each(["{bad json", "{}"])(
    "does not enroll existing users with unusable tip state %s",
    async (saved) => {
      localStorage.setItem("clipstudio-open:usage-tips", saved);
      renderWithProviders(<App />);
      await screen.findByTestId("app-layout");
      expect(loadTipsState().pending).toBe(false);
    },
  );

  it("does not replay tips already presented", async () => {
    localStorage.setItem(
      "clipstudio-open:usage-tips",
      JSON.stringify({ pending: false, presented: true }),
    );
    renderWithProviders(<App />);
    await screen.findByTestId("app-layout");
    expect(loadTipsState().pending).toBe(false);
  });

  beforeEach(() => {
    resetAppStore();
    mocks.listeners.clear();
    mocks.modelStatus.mockResolvedValue({ ready: true });
    mocks.libraryList.mockResolvedValue([library]);
    mocks.libraryCurrent.mockResolvedValue(library);
    mocks.librarySwitch.mockResolvedValue(library);
    mocks.libraryReauthorize.mockResolvedValue(undefined);
    mocks.libraryDelete.mockReset().mockResolvedValue(undefined);
    mocks.libraryImport.mockReset();
    mocks.libraryCreate.mockReset();
    mocks.openDialog.mockResolvedValue(library.path);
    mocks.settingsGet.mockResolvedValue(settings);
    mocks.onboardingGet.mockResolvedValue(completedOnboarding);
    mocks.onboardingSet.mockImplementation((currentStep: 1 | 2 | 3, completed: boolean) =>
      Promise.resolve({ version: 1, currentStep, completed }),
    );
  });

  it("loads startup state before rendering the main application", async () => {
    renderWithProviders(<App />);

    expect(screen.getByText("ClipSeek Open…")).toBeInTheDocument();
    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();

    expect(useAppStore.getState()).toMatchObject({
      loaded: true,
      libraries: [library],
      current: library,
      settings,
    });
    expect(mocks.applyTheme).toHaveBeenCalledWith("light");
    expect(loadTipsState().pending).toBe(false);
  });

  it("shows the failed library identity and three recovery actions without removing its registration", async () => {
    mocks.librarySwitch.mockRejectedValue({
      code: "library_dir_invalid",
      message: "missing .clipstudio",
    });
    renderWithProviders(<App />);
    expect(
      await screen.findByRole("heading", { name: "Unable to open your last library" }),
    ).toBeVisible();
    expect(screen.getByText(library.name)).toBeVisible();
    expect(screen.getByText(library.path)).toBeVisible();
    expect(screen.getByRole("button", { name: "Choose another library" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "New library" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled();
    expect(mocks.libraryDelete).not.toHaveBeenCalled();
    expect(mocks.onboardingSet).not.toHaveBeenCalled();
  });

  it("switches to an existing library before removing only the failed registration", async () => {
    const replacement = unavailableLibrary();
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Choose another library" }));
    await user.click(screen.getByRole("button", { name: /Archive/ }));
    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();
    expect(mocks.libraryDelete).toHaveBeenCalledExactlyOnceWith(library.id, false);
    expect(useAppStore.getState().libraries).toEqual([replacement]);
    expect(mocks.onboardingSet).not.toHaveBeenCalled();
  });

  it("keeps the original registration if the selected library cannot open", async () => {
    unavailableLibrary();
    mocks.librarySwitch.mockRejectedValue({
      code: "library_dir_invalid",
      message: "missing directory",
    });
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Choose another library" }));
    await user.click(screen.getByRole("button", { name: /Archive/ }));
    expect(
      await screen.findByRole("heading", { name: "Unable to open your last library" }),
    ).toBeVisible();
    await waitFor(() => expect(screen.getAllByRole("alert")).toHaveLength(2));
    expect(mocks.libraryDelete).not.toHaveBeenCalled();
    expect(screen.queryByTestId("app-layout")).not.toBeInTheDocument();
  });

  it("keeps the original registration when folder selection is cancelled", async () => {
    unavailableLibrary();
    mocks.openDialog.mockResolvedValue(null);
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Choose another library" }));
    await user.click(screen.getByRole("button", { name: "Browse folders" }));
    await waitFor(() => expect(mocks.openDialog).toHaveBeenCalled());
    expect(mocks.libraryImport).not.toHaveBeenCalled();
    expect(mocks.libraryDelete).not.toHaveBeenCalled();
  });

  it("opens an unregistered directory before cleaning up the failed library", async () => {
    unavailableLibrary();
    mocks.openDialog.mockResolvedValue("/library/new-folder");
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Choose another library" }));
    await user.click(screen.getByRole("button", { name: "Browse folders" }));
    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();
    expect(mocks.libraryImport).toHaveBeenCalledExactlyOnceWith("/library/new-folder");
    expect(mocks.libraryDelete).toHaveBeenCalledExactlyOnceWith(library.id, false);
  });

  it("keeps the original registration if importing the chosen directory fails", async () => {
    unavailableLibrary();
    mocks.libraryImport.mockRejectedValue(new Error("Directory cannot be opened"));
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Choose another library" }));
    await user.click(screen.getByRole("button", { name: "Browse folders" }));
    expect(await screen.findByText("Directory cannot be opened")).toBeVisible();
    expect(mocks.libraryDelete).not.toHaveBeenCalled();
  });

  it("allows cancelling creation without removing the original library", async () => {
    unavailableLibrary();
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "New library" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("heading", { name: "Unable to open your last library" })).toBeVisible();
    expect(mocks.libraryCreate).not.toHaveBeenCalled();
    expect(mocks.libraryDelete).not.toHaveBeenCalled();
  });

  it("creates and opens a replacement before removing the original registration", async () => {
    unavailableLibrary();
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "New library" }));
    await screen.findByDisplayValue("/home/test");
    await user.type(screen.getByPlaceholderText("e.g. My Design Assets"), "Archive");
    await user.click(screen.getByRole("button", { name: "Create Library" }));
    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();
    expect(mocks.libraryCreate).toHaveBeenCalledWith(
      "Archive",
      "/home/test/Archive",
      expect.any(String),
    );
    expect(mocks.libraryDelete).toHaveBeenCalledExactlyOnceWith(library.id, false);
  });

  it("retains the original library when creation fails", async () => {
    unavailableLibrary();
    mocks.libraryCreate.mockRejectedValue(new Error("Cannot create directory"));
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "New library" }));
    await screen.findByDisplayValue("/home/test");
    await user.type(screen.getByPlaceholderText("e.g. My Design Assets"), "Archive");
    await user.click(screen.getByRole("button", { name: "Create Library" }));
    expect(await screen.findByText("Cannot create directory")).toBeVisible();
    expect(mocks.libraryDelete).not.toHaveBeenCalled();
  });

  it("waits for the replacement to open and can retry a failed registration cleanup", async () => {
    let finish!: () => void;
    unavailableLibrary(
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
    );
    mocks.libraryDelete.mockRejectedValueOnce(new Error("Cannot save registry"));
    renderWithProviders(<App />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Choose another library" }));
    await user.click(screen.getByRole("button", { name: /Archive/ }));
    expect(mocks.libraryDelete).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "New library" })).toBeDisabled();
    await act(async () => finish());
    expect(await screen.findByText(/original registration could not be removed/)).toBeVisible();
    expect(screen.queryByTestId("app-layout")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();
    expect(mocks.libraryDelete).toHaveBeenCalledTimes(2);
    expect(mocks.librarySwitch.mock.calls.filter(([id]) => id === library.id)).toHaveLength(1);
  });

  it("reopens the registered library and recovers directory access before rendering assets", async () => {
    mocks.librarySwitch
      .mockRejectedValueOnce({
        code: "library_database_open_failed",
        message: `failed to open database ${library.path}/.clipstudio/library.db`,
      })
      .mockResolvedValue(library);
    renderWithProviders(<App />);
    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();
    expect(mocks.openDialog).toHaveBeenCalledWith(
      expect.objectContaining({ directory: true, defaultPath: library.path }),
    );
    expect(mocks.libraryReauthorize).toHaveBeenCalledWith(library.id, library.path);
    expect(mocks.librarySwitch).toHaveBeenCalledTimes(2);
  });

  it("keeps failed library recovery retryable without displaying an unusable asset page", async () => {
    mocks.librarySwitch
      .mockRejectedValueOnce({ code: "library_database_open_failed", message: library.path })
      .mockResolvedValue(library);
    mocks.openDialog.mockResolvedValueOnce(null);
    renderWithProviders(<App />);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.queryByTestId("app-layout")).not.toBeInTheDocument();
    expect(mocks.onboardingSet).not.toHaveBeenCalled();
    await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();
  });

  it("renders onboarding when setup is incomplete", async () => {
    mocks.onboardingGet.mockResolvedValue({
      version: 1,
      currentStep: 1,
      completed: false,
    });

    renderWithProviders(<App />);

    expect(await screen.findByTestId("first-run-wizard")).toHaveAttribute("data-step", "1");
  });

  it("allows browsing without models after onboarding is complete", async () => {
    mocks.modelStatus.mockResolvedValue({ ready: false });

    renderWithProviders(<App />);

    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();
    expect(screen.queryByTestId("model-download-gate")).not.toBeInTheDocument();
  });

  it("repairs completed onboarding when the current library is missing", async () => {
    mocks.libraryCurrent.mockResolvedValue(null);

    renderWithProviders(<App />);

    expect(await screen.findByTestId("first-run-wizard")).toHaveAttribute("data-step", "2");
    expect(mocks.onboardingSet).toHaveBeenCalledWith(2, false);
  });

  it("continues at import when a library was created before the step was persisted", async () => {
    mocks.onboardingGet.mockResolvedValue({
      version: 1,
      currentStep: 2,
      completed: false,
    });

    renderWithProviders(<App />);

    expect(await screen.findByTestId("first-run-wizard")).toHaveAttribute("data-step", "3");
    expect(mocks.onboardingSet).toHaveBeenCalledWith(3, false);
  });

  it("synchronizes the store and invalidates cached queries after a library switch", async () => {
    const switchedLibrary: LibraryMeta = {
      id: "library-2",
      name: "Archive",
      path: "/library/archive",
      createdAt: 2,
    };
    mocks.libraryList
      .mockResolvedValueOnce([library])
      .mockResolvedValue([library, switchedLibrary]);

    const { queryClient } = renderWithProviders(<App />);
    const previousKey = queryKeys.assets.pages(library.id, {});
    const nextKey = queryKeys.assets.pages(switchedLibrary.id, {});
    queryClient.setQueryData(previousKey, { source: library.id });
    queryClient.setQueryData(nextKey, { source: switchedLibrary.id });
    await screen.findByTestId("app-layout");
    await waitFor(() => expect(mocks.listeners.has("library://switched")).toBe(true));

    await act(async () => {
      mocks.listeners.get("library://switched")?.({ payload: switchedLibrary });
    });

    expect(useAppStore.getState().current).toEqual(switchedLibrary);
    await waitFor(() => {
      expect(useAppStore.getState().libraries).toEqual([library, switchedLibrary]);
    });
    expect(queryClient.getQueryState(previousKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(nextKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryData(previousKey)).toEqual({ source: library.id });
    expect(queryClient.getQueryData(nextKey)).toEqual({ source: switchedLibrary.id });
  });

  it("shows a retryable error when startup data cannot be loaded", async () => {
    const user = userEvent.setup();
    mocks.libraryList
      .mockRejectedValueOnce(new Error("library registry unavailable"))
      .mockResolvedValue([library]);

    renderWithProviders(<App />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "ClipSeek Open could not load its startup data",
    );
    await user.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByTestId("app-layout")).toBeInTheDocument();
    expect(mocks.libraryList).toHaveBeenCalledTimes(2);
  });
});
