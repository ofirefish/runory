export type AuthMethod = "password" | "privateKey";
export type KeySource = { type: "file"; path: string } | { type: "vault"; keyId: string };
export type ConnectionRoute =
  | { type: "direct" }
  | { type: "jumpHost"; profileId: string }
  | {
      type: "bastion";
      bastionId: string;
      provider: string;
      assetId: string;
      accountId?: string;
      /** JumpServer Web/API base, e.g. http://host:61080 */
      apiBaseUrl?: string;
      /** JumpServer organization id (X-JMS-ORG). Defaults to Default org when omitted. */
      orgId?: string;
      /** Absolute path to vendor CLI (`tsh` / `boundary`). Empty = PATH lookup. */
      cliPath?: string;
      /** Teleport cluster name (optional). */
      clusterName?: string;
      /** Teleport: allow insecure TLS (dev only). */
      insecure?: boolean;
    };
export type OsDistribution = "ubuntu" | "debian" | "fedora" | "centos" | "red-hat" | "rocky-linux" | "alma-linux" | "arch-linux" | "manjaro" | "open-suse" | "alpine-linux" | "amazon-linux" | "oracle-linux" | "linux-mint" | "kali-linux" | "gentoo" | "void-linux" | "nix-os" | "mac-os" | "free-bsd" | "linux";
export type ServerProfile = { id: string; name: string; host: string; port: number; username: string; groupId: string | null; authMethod: AuthMethod; keySource?: KeySource; connectionRoute: ConnectionRoute; sortOrder: number; createdAt: string; updatedAt: string; lastConnectedAt?: string; osDistribution?: OsDistribution };
export type HostGroup = { id: string; name: string; sortOrder: number; collapsed: boolean; createdAt: string; updatedAt: string };
export type CreateGroupRequest = { name: string };
export type UpdateGroupRequest = Pick<HostGroup, "id" | "name" | "sortOrder" | "collapsed">;
export type CreateProfileRequest = Pick<ServerProfile, "name" | "host" | "port" | "username" | "groupId" | "authMethod" | "keySource" | "connectionRoute">;
export type UpdateProfileRequest = Pick<ServerProfile, "id" | "name" | "host" | "port" | "username" | "groupId" | "authMethod" | "keySource" | "connectionRoute" | "sortOrder">;
export type ReorderGroupsRequest = { orderedIds: string[] };
export type ReorderProfilesRequest = { groupId: string | null; orderedIds: string[] };
