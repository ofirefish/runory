export type TunnelRule = {
  id: string;
  name: string;
  profileId: string;
  targetHost: string;
  targetPort: number;
  localPort: number;
};
export type SaveTunnelRequest = Omit<TunnelRule, "id"> & { id?: string };
export type TunnelState = "stopped" | "running" | "interrupted" | "error";
export type TunnelHealth = "unchecked" | "reachable" | "unreachable";
export type TunnelStatus = {
  state: TunnelState;
  sessionId: string | null;
  startedAt: number | null;
  checkedAt: number | null;
  health: TunnelHealth;
  errorCode: string | null;
  activeConnections: number;
  bytesSent: number;
  bytesReceived: number;
  events: { at: number; code: string }[];
};
export type TunnelView = { rule: TunnelRule; status: TunnelStatus };
export const localEndpoint = (rule: TunnelRule) => `127.0.0.1:${rule.localPort}`;
export const targetEndpoint = (rule: TunnelRule) => `${rule.targetHost.includes(":") ? `[${rule.targetHost}]` : rule.targetHost}:${rule.targetPort}`;
