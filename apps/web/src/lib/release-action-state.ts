export type ReleaseActionState = {
  code:
    | "idle"
    | "invalid"
    | "forbidden"
    | "configuration"
    | "unavailable"
    | "conflict"
    | "updated"
    | "created"
    | "missing";
  id?: string;
};

export const initialReleaseActionState: ReleaseActionState = { code: "idle" };
