import Image from "next/image";
import { cn } from "@/lib/utils";

export function BrandMark({ className }: { className?: string }) {
  return <Image src="/runory-logo.png" width={48} height={48} alt="" className={cn("size-9 rounded-xl", className)} aria-hidden="true" />;
}

