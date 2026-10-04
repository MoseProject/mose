// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { detectLocale, setLocale, t } from "../i18n";
import type { PrefsView, UpdateCheckEvent } from "../types";

let emitUpdate: (event: UpdateCheckEvent) => void = () => {};
const stopListening = vi.fn();

vi.mock("../ipc", () => ({
  api: {
    getPrefs: vi.fn(),
    getQueuedReportCount: vi.fn(),
    getGpuEncoderPreference: vi.fn().mockResolvedValue("auto"),
    setGpuEncoderPreference: vi.fn().mockResolvedValue(undefined),
    setPrefs: vi.fn().mockResolvedValue(undefined),
    setErrorReportingConsent: vi.fn().mockResolvedValue(undefined),
    deleteQueuedReports: vi.fn().mockResolvedValue(undefined),
    checkForUpdate: vi.fn().mockResolvedValue(undefined),
    installAvailableUpdate: vi.fn().mockResolvedValue(undefined),
    restartAfterUpdate: vi.fn().mockResolvedValue(undefined),
  },
  onUpdateCheck: vi.fn((handler: (event: UpdateCheckEvent) => void) => {
    emitUpdate = handler;
    return Promise.resolve(stopListening);
  }),
}));

const { Prefs } = await import("./Prefs");
const { api } = await import("../ipc");

const prefs = (): PrefsView =>
  ({
    locale: "en",
    lufsProfile: 0,
    outputFolder: "C:/out",
    autosaveMinutes: 5,
    whisperModelPath: "w",
    reframeModelPath: "r",
    backgroundRemovalModelPath: "b",
    ttsModelPath: "t",
    soundLibraryPath: "s",
    telemetryEnabled: false,
    errorReportingConsent: "disabled",
  }) as unknown as PrefsView;

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getPrefs).mockResolvedValue(prefs());
  vi.mocked(api.getQueuedReportCount).mockResolvedValue(0);
});
afterEach(() => {
  cleanup();
  setLocale(detectLocale());
});

async function open() {
  render(<Prefs />);
  await screen.findByTestId("screen-prefs");
}

const lastSaved = () => vi.mocked(api.setPrefs).mock.calls.at(-1)![0];

describe("preferences screen", () => {
  it("renders nothing until the preferences load", async () => {
    vi.mocked(api.getPrefs).mockResolvedValue(undefined as never);
    const { container } = render(<Prefs />);
    await waitFor(() => expect(api.getPrefs).toHaveBeenCalled());
    expect(container.innerHTML).toBe("");
  });

  it("saves each edited field with the rest of the preferences", async () => {
    await open();
    const [language, lufs] = screen.getAllByRole("combobox");
    fireEvent.change(language, { target: { value: "pt_br" } });
    expect(lastSaved()).toMatchObject({ locale: "pt_br" });
    // Switching language retranslates the screen immediately.
    expect(screen.getByRole("heading").textContent).toBe(t("preferences"));

    fireEvent.change(lufs, { target: { value: "2" } });
    expect(lastSaved()).toMatchObject({ lufsProfile: 2, locale: "pt_br" });

    const text = (label: string) =>
      screen.getByText(label).querySelector("input") as HTMLInputElement;
    const edits: [string, string][] = [
      ["outputFolder", "D:/new"],
      ["whisperModelPath", "w2"],
      ["reframeModelPath", "r2"],
      ["backgroundRemovalModelPath", "b2"],
      ["ttsModelPath", "t2"],
      ["soundLibraryPath", "s2"],
    ];
    for (const [key, value] of edits) {
      fireEvent.change(text(t(key as never)), { target: { value } });
      expect(lastSaved()).toMatchObject({ [key]: value });
    }
  });

  it("clamps an empty autosave interval to one minute and toggles telemetry", async () => {
    await open();
    const spin = screen.getByRole("spinbutton");
    fireEvent.change(spin, { target: { value: "12" } });
    expect(lastSaved()).toMatchObject({ autosaveMinutes: 12 });
    fireEvent.change(spin, { target: { value: "0" } });
    expect(lastSaved()).toMatchObject({ autosaveMinutes: 1 });

    const [telemetry] = screen.getAllByRole("checkbox");
    fireEvent.click(telemetry);
    expect(lastSaved()).toMatchObject({ telemetryEnabled: true });
  });

  it("stores the error reporting consent separately", async () => {
    await open();
    const [, consent] = screen.getAllByRole("checkbox") as HTMLInputElement[];
    fireEvent.click(consent);
    expect(api.setErrorReportingConsent).toHaveBeenCalledWith("always_send");
    expect(api.setPrefs).not.toHaveBeenCalled();
    fireEvent.click(consent);
    expect(api.setErrorReportingConsent).toHaveBeenLastCalledWith("disabled");
  });

  it("offers to delete queued reports only when some exist", async () => {
    await open();
    expect(screen.queryByText(t("deleteQueuedReports"))).toBeNull();
    cleanup();

    vi.mocked(api.getQueuedReportCount).mockResolvedValue(3);
    await open();
    fireEvent.click(await screen.findByText(t("deleteQueuedReports")));
    await waitFor(() => expect(screen.queryByText(t("deleteQueuedReports"))).toBeNull());
    expect(api.deleteQueuedReports).toHaveBeenCalled();
  });

  it("reports the update check outcomes", async () => {
    await open();
    fireEvent.click(screen.getByText(t("checkForUpdate")));
    expect(api.checkForUpdate).toHaveBeenCalled();

    act(() => emitUpdate({ type: "upToDate" }));
    expect(screen.getByText(t("updateCheckUpToDate"))).toBeTruthy();
    act(() => emitUpdate({ type: "failed" }));
    expect(screen.getByText(t("updateCheckFailed"))).toBeTruthy();
    act(() => emitUpdate({ type: "installFailed" }));
    expect(screen.getByText(t("updateInstallFailed"))).toBeTruthy();
  });

  it("installs an available update in place, or links to the release page", async () => {
    await open();
    act(() =>
      emitUpdate({
        type: "newerVersionAvailable",
        version: "2.0.0",
        htmlUrl: "https://example.test/r",
        autoUpdateAvailable: true,
      }),
    );
    fireEvent.click(screen.getByText(t("updateInstall")));
    expect(api.installAvailableUpdate).toHaveBeenCalledWith("2.0.0");

    act(() =>
      emitUpdate({
        type: "newerVersionAvailable",
        version: "2.0.0",
        htmlUrl: "https://example.test/r",
        autoUpdateAvailable: false,
      }),
    );
    const link = screen.getByText(t("updateOpenRelease")) as HTMLAnchorElement;
    expect(link.href).toBe("https://example.test/r");
  });

  it("restarts after an installed update and stops listening on unmount", async () => {
    const { unmount } = render(<Prefs />);
    await screen.findByTestId("screen-prefs");
    act(() => emitUpdate({ type: "installed", version: "2.0.0" }));
    fireEvent.click(screen.getByText(t("updateRestart")));
    expect(api.restartAfterUpdate).toHaveBeenCalled();

    unmount();
    await waitFor(() => expect(stopListening).toHaveBeenCalled());
  });
});

describe("preferences screen: GPU encoder", () => {
  beforeEach(() => {
    vi.mocked(api.getPrefs).mockResolvedValue(prefs());
    vi.mocked(api.getQueuedReportCount).mockResolvedValue(0);
  });
  afterEach(() => cleanup());

  it("adopts the stored preference and saves a change", async () => {
    vi.mocked(api.getGpuEncoderPreference).mockResolvedValueOnce("nvenc");
    render(<Prefs />);
    const select = (await screen.findByLabelText(t("gpuEncoder"))) as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe("nvenc"));
    fireEvent.change(select, { target: { value: "cpu" } });
    expect(api.setGpuEncoderPreference).toHaveBeenCalledWith("cpu");
  });
});
