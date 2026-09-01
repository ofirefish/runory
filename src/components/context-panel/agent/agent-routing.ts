export function inferDiagnosticInputs(text: string) {
  const url = text.match(/https?:\/\/[^\s<>'"]+/i)?.[0] ?? null;
  const service = text.match(/(?:service|systemd|服务)\s*[:：]?\s*([a-zA-Z0-9_.@-]+)/i)?.[1] ?? null;
  return { url, service, includeNginxTest: /nginx|网站|website|gateway|反向代理/i.test(text) };
}
