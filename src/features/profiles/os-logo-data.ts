import type { OsDistribution } from "../../types/domain";
import almaLinux from "../../assets/os-logos/alma-linux.svg?url";
import alpineLinux from "../../assets/os-logos/alpine-linux.svg?url";
import amazonLinux from "../../assets/os-logos/amazon-linux.svg?url";
import archLinux from "../../assets/os-logos/arch-linux.svg?url";
import centos from "../../assets/os-logos/centos.svg?url";
import debian from "../../assets/os-logos/debian.svg?url";
import fedora from "../../assets/os-logos/fedora.svg?url";
import freeBsd from "../../assets/os-logos/free-bsd.svg?url";
import gentoo from "../../assets/os-logos/gentoo.svg?url";
import kaliLinux from "../../assets/os-logos/kali-linux.svg?url";
import linux from "../../assets/os-logos/linux.svg?url";
import linuxMint from "../../assets/os-logos/linux-mint.svg?url";
import macOs from "../../assets/os-logos/mac-os.svg?url";
import manjaro from "../../assets/os-logos/manjaro.svg?url";
import nixOs from "../../assets/os-logos/nix-os.svg?url";
import openSuse from "../../assets/os-logos/open-suse.svg?url";
import oracleLinux from "../../assets/os-logos/oracle-linux.svg?url";
import redHat from "../../assets/os-logos/red-hat.svg?url";
import rockyLinux from "../../assets/os-logos/rocky-linux.svg?url";
import ubuntu from "../../assets/os-logos/ubuntu.svg?url";
import voidLinux from "../../assets/os-logos/void-linux.svg?url";

export type LogoDefinition = { label: string; asset: string; color: string };

export const osLogoDictionary: Record<OsDistribution, LogoDefinition> = {
  ubuntu: { label: "Ubuntu", asset: ubuntu, color: "#e95420" },
  debian: { label: "Debian", asset: debian, color: "#a81d33" },
  fedora: { label: "Fedora", asset: fedora, color: "#51a2da" },
  centos: { label: "CentOS", asset: centos, color: "#932279" },
  "red-hat": { label: "Red Hat", asset: redHat, color: "#ee0000" },
  "rocky-linux": { label: "Rocky Linux", asset: rockyLinux, color: "#10b981" },
  "alma-linux": { label: "AlmaLinux", asset: almaLinux, color: "#f59e0b" },
  "arch-linux": { label: "Arch Linux", asset: archLinux, color: "#1793d1" },
  manjaro: { label: "Manjaro", asset: manjaro, color: "#35bf5c" },
  "open-suse": { label: "openSUSE", asset: openSuse, color: "#73ba25" },
  "alpine-linux": { label: "Alpine Linux", asset: alpineLinux, color: "#0d597f" },
  "amazon-linux": { label: "Amazon Linux", asset: amazonLinux, color: "#ff9900" },
  "oracle-linux": { label: "Oracle Linux", asset: oracleLinux, color: "#c74634" },
  "linux-mint": { label: "Linux Mint", asset: linuxMint, color: "#87cf3e" },
  "kali-linux": { label: "Kali Linux", asset: kaliLinux, color: "#557c94" },
  gentoo: { label: "Gentoo", asset: gentoo, color: "#54487a" },
  "void-linux": { label: "Void Linux", asset: voidLinux, color: "#478061" },
  "nix-os": { label: "NixOS", asset: nixOs, color: "#5277c3" },
  "mac-os": { label: "macOS", asset: macOs, color: "#a3a3a3" },
  "free-bsd": { label: "FreeBSD", asset: freeBsd, color: "#ab2b28" },
  linux: { label: "Linux", asset: linux, color: "#f4c430" },
};
