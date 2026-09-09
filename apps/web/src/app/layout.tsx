import type { Metadata } from "next";
import { headers } from "next/headers";
import Script from "next/script";
import "./globals.css";

const googleAnalyticsId = "G-40R9WPW20D";

export const metadata: Metadata = {
  metadataBase: new URL("https://runory.app"),
  title: "Runory — Local-first Infrastructure Workspace",
  description: "Runory is a local-first, cross-platform SSH and infrastructure management workspace.",
  icons: { icon: "/runory-logo.png" },
  applicationName: "Runory",
  authors: [{ name: "Runory" }],
  creator: "Runory",
  publisher: "Runory",
  category: "developer tools",
  robots: { index: true, follow: true, googleBot: { index: true, follow: true, "max-image-preview": "large", "max-snippet": -1, "max-video-preview": -1 } },
};

export default async function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const locale = (await headers()).get("x-runory-locale") === "en-US" ? "en-US" : "zh-CN";
  return (
    <html lang={locale} className="dark">
      <body>
        {children}
        <Script
          src={`https://www.googletagmanager.com/gtag/js?id=${googleAnalyticsId}`}
          strategy="afterInteractive"
        />
        <Script id="google-analytics" strategy="afterInteractive">
          {`window.dataLayer = window.dataLayer || [];
function gtag(){dataLayer.push(arguments);}
gtag('js', new Date());
gtag('config', '${googleAnalyticsId}');`}
        </Script>
      </body>
    </html>
  );
}
