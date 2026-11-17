import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { PageContext } from "../components/context";
import { DEFAULT_SETTINGS, validateSettings, type ValidSettings } from "../model/schema";
import { stagedValue, useSettingsDialog } from "../model/staging";
import { VCS_PAGES } from "../pages/vcs";

const mocks = vi.hoisted(() => ({ roots: vi.fn() }));

vi.mock("@/platform/ipc", async (importOriginal) => {
  const real = await importOriginal<typeof import("@/platform/ipc")>();
  return {
    ...real,
    isTauri: () => true,
    commands: { ...real.commands, vcsRoots: mocks.roots },
  };
});

const ROOT = "/work/app";

function root(path: string) {
  return { root: path, operation: null, head: "main", headCommit: "abc" };
}

/** The project's effective settings as the backend would send them. */
function applySettings(settings: ValidSettings) {
  useProject.setState({ settings: settings as unknown as Settings });
  useSettingsDialog.setState({ base: settings, staged: {} });
}

beforeEach(() => {
  mocks.roots.mockReset();
  mocks.roots.mockResolvedValue({ status: "ok", data: [root(ROOT), root(`${ROOT}/services/api`)] });
  useProject.setState({ project: { root: ROOT, name: "app", isGit: true } as never });
  applySettings(validateSettings({}));
  useSettingsDialog.setState({ globalBase: structuredClone(DEFAULT_SETTINGS), newProjects: false });
});
afterEach(cleanup);

function renderMappings() {
  const page = VCS_PAGES.find((p) => p.id === "vcs.mappings")!;
  const option = page.sections?.[0]?.options[0];
  if (option?.kind !== "custom") throw new Error("Directory Mappings is not a custom control");
  const Table = option.component;
  render(
    <PageContext.Provider value={{ pageId: page.id, scope: "project" }}>
      <Table />
    </PageContext.Provider>,
  );
}

describe("Directory Mappings", () => {
  it("lists the project's repository and the nested ones, each switchable", async () => {
    renderMappings();
    expect(await screen.findByText("services/api")).toBeTruthy();
    // The project folder's repository is not a mapping: its row has no switch.
    expect(screen.getAllByRole("button", { name: /^VCS of/ })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "VCS of services/api" }));
    fireEvent.click(screen.getByRole("option", { name: "<none>" }));
    expect(stagedValue(useSettingsDialog.getState(), "project", "vcs.unregisteredRoots")).toEqual(["services/api"]);
  });

  it("keeps an unregistered folder's row and registers it again", async () => {
    applySettings(validateSettings({ vcs: { unregisteredRoots: ["services/api"] } }));
    mocks.roots.mockResolvedValue({ status: "ok", data: [root(ROOT)] });
    renderMappings();
    await waitFor(() => expect(mocks.roots).toHaveBeenCalled());
    // No repository lists it any more, so its row comes from the setting.
    expect(await screen.findByText("services/api")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "VCS of services/api" }));
    fireEvent.click(screen.getByRole("option", { name: "Git" }));
    expect(stagedValue(useSettingsDialog.getState(), "project", "vcs.unregisteredRoots")).toEqual([]);
  });
});
