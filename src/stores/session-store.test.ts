import { beforeEach, describe, expect, it } from "vitest";
import { useSessionStore, type SessionTab } from "./session-store";

const tab = (id: string, profileId = `profile-${id}`): SessionTab => ({
  id,
  profileId,
  sessionId: null,
  connectionAttemptId: `attempt-${id}`,
  state: "connecting",
  view: "terminal",
});

describe("session store", () => {
  beforeEach(() => {
    useSessionStore.setState({ tabs: [], activeTabId: null });
  });

  it("keeps independent UI metadata for concurrent sessions", () => {
    const store = useSessionStore.getState();
    store.addTab(tab("one"));
    store.addTab(tab("two"));
    store.attachSession("one", "attempt-one", "session-one");
    store.attachSession("two", "attempt-two", "session-two");

    expect(useSessionStore.getState().tabs).toMatchObject([
      { id: "one", sessionId: "session-one", state: "connected" },
      { id: "two", sessionId: "session-two", state: "connected" },
    ]);
  });

  it("ignores events from an obsolete connection attempt after reconnect", () => {
    const store = useSessionStore.getState();
    store.addTab(tab("one"));
    store.attachSession("one", "attempt-one", "old-session");
    store.beginReconnect("one", "attempt-new");
    store.attachSession("one", "attempt-new", "new-session");
    store.markClosed("one", "attempt-one");

    expect(useSessionStore.getState().tabs[0]).toMatchObject({
      sessionId: "new-session",
      connectionAttemptId: "attempt-new",
      state: "connected",
    });
  });

  it("preserves each session view across activation, disconnect and reconnect", () => {
    const store = useSessionStore.getState();
    store.addTab(tab("one", "shared-profile"));
    store.addTab(tab("two", "shared-profile"));
    store.setView("two", "files");
    store.setActive("one");
    store.markClosed("two", "attempt-two");
    store.beginReconnect("two", "attempt-new");
    store.attachSession("two", "attempt-new", "session-new");

    expect(useSessionStore.getState().tabs).toMatchObject([
      { id: "one", view: "terminal" },
      { id: "two", view: "files", state: "connected" },
    ]);
    expect(useSessionStore.getState().activeTabId).toBe("two");
  });

  it("selects an adjacent tab when the active tab closes", () => {
    const store = useSessionStore.getState();
    store.addTab(tab("one"));
    store.addTab(tab("two"));
    store.addTab(tab("three"));
    store.setActive("two");
    store.removeTab("two");

    expect(useSessionStore.getState()).toMatchObject({ activeTabId: "three" });
  });

  it("marks only the matching transport attempt as failed", () => {
    const store = useSessionStore.getState();
    store.addTab(tab("one"));
    store.attachSession("one", "attempt-one", "session-one");
    store.markError("one", "obsolete-attempt");
    expect(useSessionStore.getState().tabs[0].state).toBe("connected");

    store.markError("one", "attempt-one");
    expect(useSessionStore.getState().tabs[0]).toMatchObject({ sessionId: null, state: "error" });
  });

  it("renames only the selected tab", () => {
    const store = useSessionStore.getState();
    store.addTab(tab("one"));
    store.addTab(tab("two"));
    store.renameTab("one", "Production API");

    expect(useSessionStore.getState().tabs).toMatchObject([
      { id: "one", title: "Production API" },
      { id: "two" },
    ]);
    expect(useSessionStore.getState().tabs[1].title).toBeUndefined();
  });
});
