import { Toaster as Sonner, type ToasterProps } from "sonner";
import { useSettingsStore } from "../../stores/settings-store";

export function Toaster(props: ToasterProps) {
  const theme = useSettingsStore((state) => state.theme);
  const toasterTheme = theme === "system" && typeof window.matchMedia !== "function" ? "light" : theme;

  return <Sonner
    theme={toasterTheme}
    position="bottom-right"
    richColors
    closeButton
    className="toaster group"
    toastOptions={{
      classNames: {
        toast: "group toast group-[.toaster]:border-[hsl(var(--border))] group-[.toaster]:bg-[hsl(var(--surface))] group-[.toaster]:text-[hsl(var(--foreground))] group-[.toaster]:shadow-lg",
        description: "group-[.toast]:text-[hsl(var(--muted))]",
        actionButton: "group-[.toast]:bg-[hsl(var(--primary))] group-[.toast]:text-white",
        cancelButton: "group-[.toast]:bg-[hsl(var(--elevated))] group-[.toast]:text-[hsl(var(--foreground))]",
      },
    }}
    {...props}
  />;
}
