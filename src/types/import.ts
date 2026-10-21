import type { AuthMethod } from "./domain";

export type CatalogImportSource = "terminus" | "openssh" | "xshell" | "securecrt" | "mobaxterm";

export type CatalogImportProfilePreview = {
  importKey: string;
  name: string;
  host: string;
  port: number;
  username: string;
  groupName: string | null;
  authMethod: AuthMethod;
  keyPath: string | null;
  hasPassword: boolean;
  importable: boolean;
  warnings: string[];
};

export type CatalogImportPreview = {
  profileCount: number;
  importableCount: number;
  profiles: CatalogImportProfilePreview[];
};

export type CatalogImportApplyResult = {
  groupsCreated: number;
  profilesCreated: number;
  profilesUpdated: number;
  jumpsWired: number;
  passwordsStored: number;
  skipped: number;
};
