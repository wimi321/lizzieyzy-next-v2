import { invoke } from "@tauri-apps/api/core";
import type { ZhiziStatus } from "./generated/ZhiziStatus";
import type { ZhiziVerificationPurpose } from "./generated/ZhiziVerificationPurpose";
import type { ZhiziCodeDelivery } from "./generated/ZhiziCodeDelivery";
import type { ZhiziOverview } from "./generated/ZhiziOverview";
import type { ZhiziUsagePage } from "./generated/ZhiziUsagePage";
import type { ZhiziCreditPage } from "./generated/ZhiziCreditPage";
export type { ZhiziStatus } from "./generated/ZhiziStatus";
export type { ZhiziVerificationPurpose } from "./generated/ZhiziVerificationPurpose";
export type { ZhiziCodeDelivery } from "./generated/ZhiziCodeDelivery";
export type { ZhiziOverview } from "./generated/ZhiziOverview";
export type { ZhiziUsagePage } from "./generated/ZhiziUsagePage";
export type { ZhiziCreditPage } from "./generated/ZhiziCreditPage";
export type { ZhiziEngineOptions } from "./generated/ZhiziEngineOptions";
export type { ZhiziPlan } from "./generated/ZhiziPlan";
export type { ZhiziWeight } from "./generated/ZhiziWeight";
export type { ZhiziError } from "./generated/ZhiziError";
export type { ZhiziErrorKind } from "./generated/ZhiziErrorKind";

/** Rust owns authentication, cooldowns, caching, and secret persistence. */
export interface ZhiziCommands {
  zhiziStatus?(): Promise<ZhiziStatus>;
  zhiziLoginPassword?(account: string, password: string, remember: boolean): Promise<ZhiziStatus>;
  zhiziLoginCode?(account: string, code: string, remember: boolean): Promise<ZhiziStatus>;
  zhiziSendCode?(account: string, purpose: ZhiziVerificationPurpose): Promise<ZhiziCodeDelivery>;
  zhiziResetPassword?(account: string, code: string, password: string, remember: boolean): Promise<ZhiziStatus>;
  zhiziLogout?(): Promise<ZhiziStatus>;
  zhiziOverview?(force: boolean): Promise<ZhiziOverview>;
  zhiziUsages?(page: number): Promise<ZhiziUsagePage>;
  zhiziCredits?(page: number): Promise<ZhiziCreditPage>;
  zhiziWebsite?(): Promise<string>;
}

export const zhiziStatus: NonNullable<ZhiziCommands["zhiziStatus"]> = () =>
  invoke("zhizi_status");
export const zhiziLoginPassword: NonNullable<ZhiziCommands["zhiziLoginPassword"]> = (account, password, remember) =>
  invoke("zhizi_login_password", { account, password, remember });
export const zhiziLoginCode: NonNullable<ZhiziCommands["zhiziLoginCode"]> = (account, code, remember) =>
  invoke("zhizi_login_code", { account, code, remember });
export const zhiziSendCode: NonNullable<ZhiziCommands["zhiziSendCode"]> = (account, purpose) =>
  invoke("zhizi_send_code", { account, purpose });
export const zhiziResetPassword: NonNullable<ZhiziCommands["zhiziResetPassword"]> = (account, code, password, remember) =>
  invoke("zhizi_reset_password", { account, code, password, remember });
export const zhiziLogout: NonNullable<ZhiziCommands["zhiziLogout"]> = () =>
  invoke("zhizi_logout");
export const zhiziOverview: NonNullable<ZhiziCommands["zhiziOverview"]> = (force) =>
  invoke("zhizi_overview", { force });
export const zhiziUsages: NonNullable<ZhiziCommands["zhiziUsages"]> = (page) =>
  invoke("zhizi_usages", { page });
export const zhiziCredits: NonNullable<ZhiziCommands["zhiziCredits"]> = (page) =>
  invoke("zhizi_credits", { page });
export const zhiziWebsite: NonNullable<ZhiziCommands["zhiziWebsite"]> = () =>
  invoke("zhizi_website");
