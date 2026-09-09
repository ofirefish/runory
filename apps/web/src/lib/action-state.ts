export type ActionState = { code: "idle" | "invalid" | "authFailed" | "configuration" | "confirmationSent" | "resetSent" | "updated" };

export const initialActionState: ActionState = { code: "idle" };
