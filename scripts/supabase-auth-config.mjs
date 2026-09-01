import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const templates = [
  ["confirmation", "Confirm your Runory email / 确认 Runory 邮箱"],
  ["invite", "You are invited to Runory / Runory 账号邀请"],
  ["recovery", "Reset your Runory password / 重置 Runory 密码"],
  ["magic_link", "Your Runory sign-in link / Runory 登录链接"],
  ["email_change", "Confirm your new Runory email / 确认新的 Runory 邮箱"],
];

const fail = (message) => {
  throw new Error(message);
};

async function loadTemplates() {
  const values = {};
  for (const [name, subject] of templates) {
    const path = `${root}supabase/templates/${name}.html`;
    const content = await readFile(path, "utf8");
    if (!content.includes("{{ .ConfirmationURL }}")) fail(`${name}: missing ConfirmationURL`);
    if (!content.includes("<!doctype html>") || !content.includes("</html>")) fail(`${name}: invalid HTML envelope`);
    if (/<script\b/i.test(content) || /https?:\/\//i.test(content)) fail(`${name}: active or remote content is not allowed`);
    if (Buffer.byteLength(content, "utf8") > 64 * 1024) fail(`${name}: template exceeds 64 KiB`);
    values[`mailer_subjects_${name}`] = subject;
    values[`mailer_templates_${name}_content`] = content;
  }
  return values;
}

function required(name) {
  const value = process.env[name];
  if (!value?.trim()) fail(`${name} is required`);
  return value;
}

function productionConfig() {
  const projectRef = required("SUPABASE_PROJECT_REF");
  if (!/^[a-z0-9]{20}$/.test(projectRef)) fail("SUPABASE_PROJECT_REF must be a 20-character project reference");
  const smtpHost = required("RUNORY_SMTP_HOST");
  if (!/^[a-z0-9.-]+$/i.test(smtpHost)) fail("RUNORY_SMTP_HOST must be a hostname without a URL scheme");
  const smtpPort = Number(required("RUNORY_SMTP_PORT"));
  if (!Number.isInteger(smtpPort) || smtpPort < 1 || smtpPort > 65535) fail("RUNORY_SMTP_PORT must be a valid TCP port");
  const adminEmail = required("RUNORY_SMTP_ADMIN_EMAIL");
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(adminEmail)) fail("RUNORY_SMTP_ADMIN_EMAIL must be a valid email address");
  return {
    accessToken: required("SUPABASE_ACCESS_TOKEN"),
    projectRef,
    auth: {
      external_email_enabled: true,
      mailer_secure_email_change_enabled: true,
      mailer_autoconfirm: false,
      smtp_admin_email: adminEmail,
      smtp_host: smtpHost,
      smtp_port: smtpPort,
      smtp_user: required("RUNORY_SMTP_USER"),
      smtp_pass: required("RUNORY_SMTP_PASS"),
      smtp_sender_name: process.env.RUNORY_SMTP_SENDER_NAME?.trim() || "Runory",
    },
  };
}

async function request(projectRef, accessToken, method, body) {
  const response = await fetch(`https://api.supabase.com/v1/projects/${projectRef}/config/auth`, {
    method,
    headers: {
      Authorization: `Bearer ${accessToken}`,
      ...(body ? { "Content-Type": "application/json" } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(15_000),
  });
  if (!response.ok) fail(`Supabase Management API returned HTTP ${response.status}`);
  return response.json();
}

function verify(actual, expected) {
  for (const [key, value] of Object.entries(expected)) {
    if (key === "smtp_pass") continue;
    if (actual[key] !== value) fail(`remote Auth configuration mismatch: ${key}`);
  }
}

async function main() {
  const mode = process.argv[2] ?? "--check";
  if (!["--check", "--apply", "--verify"].includes(mode)) fail("use --check, --apply, or --verify");
  const templateConfig = await loadTemplates();
  if (mode === "--check") {
    process.stdout.write(`Validated ${templates.length} Runory Auth email templates.\n`);
    return;
  }
  const config = productionConfig();
  const expected = { ...config.auth, ...templateConfig };
  if (mode === "--apply") {
    await request(config.projectRef, config.accessToken, "PATCH", expected);
  }
  const actual = await request(config.projectRef, config.accessToken, "GET");
  verify(actual, expected);
  process.stdout.write(`Runory Auth email configuration ${mode === "--apply" ? "applied and " : ""}verified.\n`);
}

main().catch((error) => {
  process.stderr.write(`Auth email configuration failed: ${error instanceof Error ? error.message : "unknown error"}\n`);
  process.exitCode = 1;
});
