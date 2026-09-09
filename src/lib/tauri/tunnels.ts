import { invoke } from "@tauri-apps/api/core";
import type { SaveTunnelRequest, TunnelRule, TunnelView } from "../../types/tunnels";
import type { TestConnectionRequest, TestConnectionResponse } from "../../types/session";

export const listTunnels = () => invoke<TunnelView[]>("tunnel_list");
export const sessionTunnelImpact = (sessionId: string) => invoke<TunnelRule[]>("tunnel_session_impact", { request: { sessionId } });
export const saveTunnel = (request: SaveTunnelRequest) => invoke<TunnelRule>("tunnel_save", { request });
export const deleteTunnel = (id: string) => invoke<void>("tunnel_delete", { request: { id } });
export const startTunnel = (id: string) => invoke<void>("tunnel_start", { request: { id } });
export const connectStartTunnel = (id: string, connection: TestConnectionRequest) => invoke<TestConnectionResponse>("tunnel_connect_start", { request: { id, ...connection } });
export const stopTunnel = (id: string) => invoke<void>("tunnel_stop", { request: { id } });
export const checkTunnel = (id: string) => invoke<void>("tunnel_check", { request: { id } });
