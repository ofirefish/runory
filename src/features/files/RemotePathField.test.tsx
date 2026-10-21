// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import { RemotePathField } from "./RemotePathField";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await i18n.changeLanguage("zh-CN");
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  vi.unstubAllGlobals();
});

describe("RemotePathField", () => {
  it("disables browse without a session and enables it when connected", async () => {
    const onChange = vi.fn();
    await act(async () => {
      root.render(
        <RemotePathField
          label={i18n.t("deployment.python.projectPath")}
          value="/srv/app"
          onChange={onChange}
          sessionId={null}
        />,
      );
    });
    const browse = container.querySelector<HTMLButtonElement>(`button[aria-label="${i18n.t("remoteFolderPicker.browse")}"]`);
    expect(browse?.disabled).toBe(true);

    await act(async () => {
      root.render(
        <RemotePathField
          label={i18n.t("deployment.python.projectPath")}
          value="/srv/app"
          onChange={onChange}
          sessionId="session-1"
        />,
      );
    });
    const enabled = container.querySelector<HTMLButtonElement>(`button[aria-label="${i18n.t("remoteFolderPicker.browse")}"]`);
    expect(enabled?.disabled).toBe(false);
  });
});
