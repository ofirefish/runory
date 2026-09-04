const workspaceViews = ["terminal", "files", "dashboard", "operations", "deployment"] as const;

export type WorkspaceView = typeof workspaceViews[number];

export function restoreWorkspaceView(saved: string | null): WorkspaceView {
  return workspaceViews.find((view) => view === saved) ?? "terminal";
}
