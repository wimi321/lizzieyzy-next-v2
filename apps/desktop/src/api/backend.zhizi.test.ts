import { beforeEach, expect, test, vi } from "vitest";
import type { EngineProfile } from "./backend";
const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); });

test("account adapters forward credentials unchanged and preserve typed failures", async () => {
  const api = await import("./backend");
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [api.zhiziStatus, "zhizi_status", undefined],
    [() => api.zhiziLoginPassword(" fixture@example.test ", " password ", false), "zhizi_login_password", { account: " fixture@example.test ", password: " password ", remember: false }],
    [() => api.zhiziLoginCode("fixture@example.test", "1234", true), "zhizi_login_code", { account: "fixture@example.test", code: "1234", remember: true }],
    [() => api.zhiziSendCode("fixture@example.test", "FastLogin"), "zhizi_send_code", { account: "fixture@example.test", purpose: "FastLogin" }],
    [() => api.zhiziResetPassword("fixture@example.test", "1234", "new password", false), "zhizi_reset_password", { account: "fixture@example.test", code: "1234", password: "new password", remember: false }],
    [api.zhiziLogout, "zhizi_logout", undefined],
    [() => api.zhiziOverview(false), "zhizi_overview", { force: false }],
    [() => api.zhiziUsages(2), "zhizi_usages", { page: 2 }],
    [() => api.zhiziCredits(3), "zhizi_credits", { page: 3 }],
    [api.zhiziWebsite, "zhizi_website", undefined],
  ];
  for (const [call, name, args] of cases) {
    await call();
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const error = { type: "Zhizi", data: { kind: { type: "LoginExpired" }, status: 401, request_id: "test-1" } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.zhiziOverview(true)).rejects.toBe(error);
});

test("every account mock command needs a script and old snapshots receive defaults", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const calls: [string, () => Promise<unknown>, unknown][] = [
    ["zhiziStatus", mock.zhiziStatus, {}],
    ["zhiziLoginPassword", () => mock.zhiziLoginPassword("test", " pass ", false), { logged_in: true }],
    ["zhiziLoginCode", () => mock.zhiziLoginCode("test", "1234", true), {}],
    ["zhiziSendCode", () => mock.zhiziSendCode("test", "FastLogin"), { retry_after_seconds: 90 }],
    ["zhiziResetPassword", () => mock.zhiziResetPassword("test", "1234", "password", false), {}],
    ["zhiziLogout", mock.zhiziLogout, {}],
    ["zhiziOverview", () => mock.zhiziOverview(false), {}],
    ["zhiziUsages", () => mock.zhiziUsages(2), { page: 2, items: [{}] }],
    ["zhiziCredits", () => mock.zhiziCredits(3), { page: 3, items: [{}] }],
    ["zhiziWebsite", mock.zhiziWebsite, "https://www.zhizigo.com/"],
  ];
  mock.setScript({ commands: [
    { command: "zhiziStatus", result: {} },
    { command: "zhiziLoginPassword", result: { logged_in: true } },
    { command: "zhiziLoginCode", result: {} },
    { command: "zhiziSendCode", result: { retry_after_seconds: 90 } },
    { command: "zhiziResetPassword", result: {} },
    { command: "zhiziLogout", result: {} },
    { command: "zhiziOverview", result: {} },
    { command: "zhiziUsages", result: { page: 2, items: [{}] } },
    { command: "zhiziCredits", result: { page: 3, items: [{}] } },
    { command: "zhiziWebsite", result: "https://www.zhizigo.com/" },
  ] });
  const results = [];
  for (const [, call] of calls) results.push(await call());
  expect(results[0]).toHaveProperty("remembered", false);
  expect(results[1]).toHaveProperty("logged_in", true);
  expect(results[3]).toHaveProperty("retry_after_seconds", 90);
  expect(results[6]).toHaveProperty("recent_usage.page_size", 5);
  expect(results[7]).toHaveProperty("items.0.duration_seconds", 0);
  expect(results[8]).toHaveProperty("page_size", 20);
  for (const [, call] of calls) await expect(call()).rejects.toThrow("Expected mock command");
});

test("Zhizi profile defaults remain optional and explicit plan and weight survive", async () => {
  const { fillRemoteProfileDefaults } = await import("../../e2e/fixtures");
  const profile: EngineProfile = { program: "", model: "", config: "", extra_args: [], remote: { type: "Zhizi", data: {} } };
  fillRemoteProfileDefaults(profile);
  if (profile.remote?.type !== "Zhizi") throw new Error("missing profile");
  expect(profile.remote.data.options?.plan).toBe("Vip");
  expect(profile.remote.data.options?.weight).toBe("10b512t");
  profile.remote.data.options = { plan: "OnDemand1xCuda", weight: "20b" };
  fillRemoteProfileDefaults(profile);
  expect(profile.remote.data.options).toEqual({ plan: "OnDemand1xCuda", weight: "20b" });
});
