import { z } from "zod";

export const tunnelSchema = z.object({
  name: z.string().trim().min(1).max(120).refine((value) => !/\p{Cc}/u.test(value)),
  profileId: z.string().min(1),
  targetHost: z.string().trim().min(1).max(253).refine((value) =>
    z.ipv4().safeParse(value).success || z.ipv6().safeParse(value).success ||
    value.replace(/\.$/, "").split(".").every((part) => /^[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/.test(part))),
  targetPort: z.number().int().min(1).max(65535),
  localPort: z.number().int().min(1).max(65535),
});
export type TunnelFormValues = z.infer<typeof tunnelSchema>;
