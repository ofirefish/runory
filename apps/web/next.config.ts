import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  // An isolated directory allows validation while a local preview holds build files open.
  distDir: process.env.RUNORY_WEB_BUILD_DIR || ".next",
};

export default nextConfig;
