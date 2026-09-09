import type { HostGroup, ServerProfile } from "../../types/domain";
import type { SessionTab } from "../../stores/session-store";

export const MAX_FLEET_TARGETS = 10;

export type FleetMentionKind = "server" | "group";

export type FleetMention = {
  kind: FleetMentionKind;
  name: string;
  role: string | null;
  raw: string;
  start: number;
  end: number;
};

export type FleetMentionErrorCode =
  | "unterminated-quote"
  | "invalid-role"
  | "server-not-found"
  | "server-name-ambiguous"
  | "group-not-found"
  | "group-name-ambiguous"
  | "group-empty"
  | "target-disconnected"
  | "target-session-ambiguous"
  | "target-role-conflict"
  | "too-few-targets"
  | "too-many-targets";

export type FleetMentionError = {
  code: FleetMentionErrorCode;
  mention: string;
  name: string;
};

export type FleetTargetBinding = {
  profileId: string;
  sessionId: string;
  displayName: string;
  role: string | null;
  ordinal: number;
};

export type FleetMentionParseResult = {
  mentions: FleetMention[];
  errors: FleetMentionError[];
};

export type FleetTargetResolution = {
  targets: FleetTargetBinding[];
  errors: FleetMentionError[];
};

const ROLE_CHARACTER = /[A-Za-z0-9._-]/;
const MENTION_DELIMITER = /[\s,，;；。!?！？()[\]{}<>]/;

function isMentionBoundary(text: string, index: number): boolean {
  if (index === 0) return true;
  return /[\s,，;；。!?！？()[\]{}<>]/.test(text[index - 1] ?? "");
}

function readQuotedName(text: string, start: number): { name: string; next: number } | null {
  const end = text.indexOf('"', start + 1);
  if (end < 0) return null;
  return { name: text.slice(start + 1, end).trim(), next: end + 1 };
}

function readBareName(text: string, start: number): { name: string; next: number } {
  let next = start;
  while (next < text.length && text[next] !== "#" && !MENTION_DELIMITER.test(text[next])) {
    next += 1;
  }
  return { name: text.slice(start, next).trim(), next };
}

/**
 * Parses user-facing mentions only. The returned names never authorize an
 * operation; callers must resolve them to exact IDs and Rust must validate the
 * live profile/session relationship again.
 */
export function parseFleetMentions(text: string): FleetMentionParseResult {
  const mentions: FleetMention[] = [];
  const errors: FleetMentionError[] = [];
  let index = 0;

  while (index < text.length) {
    if (text[index] !== "@" || !isMentionBoundary(text, index)) {
      index += 1;
      continue;
    }

    const start = index;
    let cursor = index + 1;
    let kind: FleetMentionKind = "server";
    if (text.slice(cursor, cursor + 6).toLocaleLowerCase() === "group:") {
      kind = "group";
      cursor += 6;
    }

    let parsedName: { name: string; next: number } | null;
    if (text[cursor] === '"') {
      parsedName = readQuotedName(text, cursor);
      if (!parsedName) {
        errors.push({ code: "unterminated-quote", mention: text.slice(start), name: "" });
        break;
      }
    } else {
      parsedName = readBareName(text, cursor);
    }

    if (!parsedName.name) {
      index += 1;
      continue;
    }
    cursor = parsedName.next;

    let role: string | null = null;
    if (text[cursor] === "#") {
      const roleStart = cursor + 1;
      cursor = roleStart;
      while (cursor < text.length && ROLE_CHARACTER.test(text[cursor])) cursor += 1;
      if (cursor === roleStart) {
        errors.push({
          code: "invalid-role",
          mention: text.slice(start, cursor),
          name: parsedName.name,
        });
        index = cursor;
        continue;
      }
      role = text.slice(roleStart, cursor).toLocaleLowerCase();
    }

    mentions.push({
      kind,
      name: parsedName.name,
      role,
      raw: text.slice(start, cursor),
      start,
      end: cursor,
    });
    index = cursor;
  }

  return { mentions, errors };
}

function matchingByName<T extends { name: string }>(items: T[], name: string): T[] {
  const normalized = name.toLocaleLowerCase();
  return items.filter((item) => item.name.toLocaleLowerCase() === normalized);
}

function liveSessions(profileId: string, tabs: SessionTab[]): SessionTab[] {
  return tabs.filter((tab) =>
    tab.profileId === profileId && tab.state === "connected" && tab.sessionId !== null,
  );
}

/**
 * Resolves parsed names against the current catalog and connected session UI
 * metadata. Ambiguity is rejected instead of picking an arbitrary tab. The
 * Rust preflight remains the authority for whether the session is still live
 * and belongs to the submitted profile.
 */
export function resolveFleetMentions(
  mentions: FleetMention[],
  profiles: ServerProfile[],
  groups: HostGroup[],
  tabs: SessionTab[],
): FleetTargetResolution {
  const errors: FleetMentionError[] = [];
  const resolved = new Map<string, FleetTargetBinding>();

  const addProfile = (profile: ServerProfile, role: string | null, raw: string) => {
    const sessions = liveSessions(profile.id, tabs);
    if (sessions.length === 0) {
      errors.push({ code: "target-disconnected", mention: raw, name: profile.name });
      return;
    }
    if (sessions.length > 1) {
      errors.push({ code: "target-session-ambiguous", mention: raw, name: profile.name });
      return;
    }
    const existing = resolved.get(profile.id);
    if (existing) {
      if (existing.role !== role && role !== null) {
        errors.push({ code: "target-role-conflict", mention: raw, name: profile.name });
      }
      return;
    }
    resolved.set(profile.id, {
      profileId: profile.id,
      sessionId: sessions[0].sessionId as string,
      displayName: profile.name,
      role,
      ordinal: resolved.size,
    });
  };

  for (const mention of mentions) {
    if (mention.kind === "server") {
      const matches = matchingByName(profiles, mention.name);
      if (matches.length === 0) {
        errors.push({ code: "server-not-found", mention: mention.raw, name: mention.name });
      } else if (matches.length > 1) {
        errors.push({ code: "server-name-ambiguous", mention: mention.raw, name: mention.name });
      } else {
        addProfile(matches[0], mention.role, mention.raw);
      }
      continue;
    }

    const matches = matchingByName(groups, mention.name);
    if (matches.length === 0) {
      errors.push({ code: "group-not-found", mention: mention.raw, name: mention.name });
      continue;
    }
    if (matches.length > 1) {
      errors.push({ code: "group-name-ambiguous", mention: mention.raw, name: mention.name });
      continue;
    }
    const members = profiles.filter((profile) => profile.groupId === matches[0].id);
    if (members.length === 0) {
      errors.push({ code: "group-empty", mention: mention.raw, name: mention.name });
      continue;
    }
    for (const profile of members) addProfile(profile, mention.role, mention.raw);
  }

  const targets = [...resolved.values()];
  if (mentions.length > 0 && targets.length < 2 && errors.length === 0) {
    errors.push({ code: "too-few-targets", mention: mentions[0].raw, name: mentions[0].name });
  }
  if (targets.length > MAX_FLEET_TARGETS) {
    errors.push({ code: "too-many-targets", mention: "", name: String(targets.length) });
    return { targets: [], errors };
  }
  return { targets, errors };
}

