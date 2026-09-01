import { invoke } from "@tauri-apps/api/core";
import type { CreateGroupRequest, CreateProfileRequest, HostGroup, ReorderGroupsRequest, ReorderProfilesRequest, ServerProfile, UpdateGroupRequest, UpdateProfileRequest } from "../../types/domain";

export const listGroups = () => invoke<HostGroup[]>("group_list");
export const createGroup = (request: CreateGroupRequest) => invoke<HostGroup>("group_create", { request });
export const updateGroup = (request: UpdateGroupRequest) => invoke<HostGroup>("group_update", { request });
export const deleteGroup = (id: string) => invoke<void>("group_delete", { request: { id } });
export const reorderGroups = (request: ReorderGroupsRequest) => invoke<HostGroup[]>("group_reorder", { request });
export const listProfiles = () => invoke<ServerProfile[]>("profile_list");
export const createProfile = (request: CreateProfileRequest) => invoke<ServerProfile>("profile_create", { request });
export const updateProfile = (request: UpdateProfileRequest) => invoke<ServerProfile>("profile_update", { request });
export const deleteProfile = (id: string) => invoke<void>("profile_delete", { request: { id } });
export const reorderProfiles = (request: ReorderProfilesRequest) => invoke<ServerProfile[]>("profile_reorder", { request });
