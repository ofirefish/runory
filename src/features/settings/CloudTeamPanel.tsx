import { Check, Trash2, UserPlus, Users } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../components/ui/button";
import { ConfirmDialog } from "../../components/ui/confirm-dialog";
import { Input } from "../../components/ui/input";
import {
  acceptOrganizationInvite,
  createOrganizationInvite,
  getMyMembership,
  listMyOrganizationInvites,
  listOrganizationInvites,
  listOrganizationMembers,
  removeOrganizationMember,
  revokeOrganizationInvite,
  updateOrganizationMemberRole,
} from "../../lib/supabase/cloud";
import type {
  MyOrganizationInvite,
  OrganizationInvite,
  OrganizationMemberDetails,
  OrganizationRole,
} from "../../types/cloud";

type InviteRole = Exclude<OrganizationRole, "owner">;

export function CloudTeamPanel({ organizationId, userId, onMembershipChanged }: {
  organizationId: string | null;
  userId: string;
  onMembershipChanged: () => Promise<void>;
}) {
  const { t } = useTranslation();
  const [role, setRole] = useState<OrganizationRole | null>(null);
  const [members, setMembers] = useState<OrganizationMemberDetails[]>([]);
  const [sentInvites, setSentInvites] = useState<OrganizationInvite[]>([]);
  const [myInvites, setMyInvites] = useState<MyOrganizationInvite[]>([]);
  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteRole, setInviteRole] = useState<InviteRole>("viewer");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [memberToRemove, setMemberToRemove] = useState<OrganizationMemberDetails | null>(null);

  const load = useCallback(async () => {
    const received = await listMyOrganizationInvites();
    setMyInvites(received);
    if (!organizationId) {
      setRole(null); setMembers([]); setSentInvites([]); return;
    }
    const membership = await getMyMembership(organizationId, userId);
    setRole(membership?.role ?? null);
    if (membership?.role === "owner" || membership?.role === "admin") {
      const [memberRows, invitationRows] = await Promise.all([
        listOrganizationMembers(organizationId),
        listOrganizationInvites(organizationId),
      ]);
      setMembers(memberRows);
      setSentInvites(invitationRows);
    } else {
      setMembers([]);
      setSentInvites([]);
    }
  }, [organizationId, userId]);

  useEffect(() => { void load().catch(() => setFailed(true)); }, [load]);

  const createInvite = async () => {
    if (!organizationId || !inviteEmail.trim()) return;
    setBusy(true); setFailed(false);
    try {
      await createOrganizationInvite(organizationId, inviteEmail.trim(), inviteRole);
      setInviteEmail(""); await load(); await onMembershipChanged();
    } catch { setFailed(true); } finally { setBusy(false); }
  };
  const acceptInvite = async (id: string) => {
    setBusy(true); setFailed(false);
    try { await acceptOrganizationInvite(id); await onMembershipChanged(); await load(); }
    catch { setFailed(true); } finally { setBusy(false); }
  };
  const revokeInvite = async (id: string) => {
    setBusy(true); setFailed(false);
    try { await revokeOrganizationInvite(id); await load(); await onMembershipChanged(); }
    catch { setFailed(true); } finally { setBusy(false); }
  };
  const updateMemberRole = async (memberId: string, nextRole: InviteRole) => {
    if (!organizationId) return;
    setBusy(true); setFailed(false);
    try { await updateOrganizationMemberRole(organizationId, memberId, nextRole); await load(); await onMembershipChanged(); }
    catch { setFailed(true); } finally { setBusy(false); }
  };
  const removeMember = async () => {
    if (!organizationId || !memberToRemove) return;
    setBusy(true); setFailed(false);
    try { await removeOrganizationMember(organizationId, memberToRemove.user_id); await load(); await onMembershipChanged(); setMemberToRemove(null); }
    catch (error) { setFailed(true); throw error; } finally { setBusy(false); }
  };

  const canManage = role === "owner" || role === "admin";
  return <section className="mt-4 border-t pt-3">
    <div className="flex items-center gap-2"><Users size={14} /><h4 className="text-xs font-medium">{t("cloud.team")}</h4></div>
    {role && <p className="mt-1 text-xs text-[hsl(var(--muted))]">{t("cloud.yourRole", { role: t(`cloud.role.${role}`) })}</p>}
    {myInvites.length > 0 && <div className="mt-3 grid gap-2"><p className="text-xs font-medium">{t("cloud.receivedInvites")}</p>{myInvites.map((invite) => <div key={invite.id} className="flex items-center justify-between gap-2 rounded border p-2 text-xs"><span>{invite.organization_name} · {t(`cloud.role.${invite.role}`)}</span><Button size="sm" disabled={busy} onClick={() => void acceptInvite(invite.id)}><Check size={13} />{t("cloud.acceptInvite")}</Button></div>)}</div>}
    {canManage && <><div className="mt-3 grid gap-2"><p className="text-xs font-medium">{t("cloud.members")}</p>{members.map((member) => { const editable = member.role !== "owner" && (role === "owner" || member.role === "operator" || member.role === "viewer"); return <div key={member.user_id} className="flex items-center justify-between gap-2 rounded border p-2 text-xs"><span className="min-w-0 flex-1 truncate">{member.email}</span>{editable ? <><select className="rounded border bg-transparent px-1 py-1 text-xs" value={member.role} disabled={busy} aria-label={t("cloud.memberRole")} onChange={(event) => void updateMemberRole(member.user_id, event.target.value as InviteRole)}>{role === "owner" && <option value="admin">{t("cloud.role.admin")}</option>}<option value="operator">{t("cloud.role.operator")}</option><option value="viewer">{t("cloud.role.viewer")}</option></select><Button size="icon" variant="ghost" disabled={busy} aria-label={t("cloud.removeMember")} onClick={() => setMemberToRemove(member)}><Trash2 size={13} /></Button></> : <span className="text-[hsl(var(--muted))]">{t(`cloud.role.${member.role}`)}</span>}</div>; })}</div>
      <div className="mt-3 grid gap-2"><p className="text-xs font-medium">{t("cloud.inviteMember")}</p><Input type="email" value={inviteEmail} onChange={(event) => setInviteEmail(event.target.value)} placeholder={t("cloud.inviteEmail")} aria-label={t("cloud.inviteEmail")} /><div className="flex gap-2"><select className="min-w-0 flex-1 rounded border bg-transparent px-2 text-xs" value={inviteRole} onChange={(event) => setInviteRole(event.target.value as InviteRole)} aria-label={t("cloud.inviteRole")}><option value="admin">{t("cloud.role.admin")}</option><option value="operator">{t("cloud.role.operator")}</option><option value="viewer">{t("cloud.role.viewer")}</option></select><Button size="sm" disabled={busy || !inviteEmail.trim()} onClick={() => void createInvite()}><UserPlus size={13} />{t("cloud.sendInvite")}</Button></div></div>
      {sentInvites.length > 0 && <div className="mt-3 grid gap-2"><p className="text-xs font-medium">{t("cloud.pendingInvites")}</p>{sentInvites.map((invite) => <div key={invite.id} className="flex items-center justify-between gap-2 rounded border p-2 text-xs"><span className="truncate">{invite.email} · {t(`cloud.role.${invite.role}`)}</span><Button size="icon" variant="ghost" disabled={busy} aria-label={t("cloud.revokeInvite")} onClick={() => void revokeInvite(invite.id)}><Trash2 size={13} /></Button></div>)}</div>}
    </>}
    {failed && <p className="mt-2 text-xs text-red-500">{t("cloud.teamError")}</p>}
    {memberToRemove && <ConfirmDialog title={t("cloud.removeMemberTitle")} description={t("cloud.removeMemberDescription", { email: memberToRemove.email })} onConfirm={removeMember} onClose={() => setMemberToRemove(null)} />}
  </section>;
}
