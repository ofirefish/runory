/** Desktop deep-link handoff after HTTPS email confirmation. Tokens stay in the fragment. */
export function buildDesktopEmailConfirmDeepLink(accessToken: string, refreshToken: string): string {
  const hash = new URLSearchParams({
    access_token: accessToken,
    refresh_token: refreshToken,
    type: "email_confirm",
  });
  return `runory://auth/callback#${hash.toString()}`;
}
