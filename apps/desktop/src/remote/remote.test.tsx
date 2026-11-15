import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useEffect } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { EngineProfile, Settings } from "@/api/backend";
import type { Engine } from "../engines/model";
import en from "../locales/en.json";
import { duration, remoteError, remoteSummary, yuan } from "./format";
import { SelfHosted } from "./SelfHosted";
import { SshEngineDialog } from "./SshEngineDialog";
import { useRemote } from "./useRemote";
import { ZhiziLogin } from "./ZhiziLogin";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
const labels = en.remote;
const CHANGED = { type: "Remote", data: { type: "HostKeyChanged", data: { old_fingerprint: "SHA256:old", new_fingerprint: "SHA256:new", path: "/data/ssh/known_hosts" } } };

test("remote failures, engines, money and time read in the interface language", () => {
  expect(remoteError(CHANGED, labels)).toBe(labels.engineErrors.HostKeyChanged);
  expect(remoteError({ type: "Remote", data: { type: "KnownHosts", data: { path: "/data/ssh/known_hosts" } } }, labels)).toBe(
    "The known hosts file could not be read or written: /data/ssh/known_hosts",
  );
  expect(remoteError({ type: "RemoteLink", data: "NoQrCode" }, labels)).toBe(labels.linkErrors.NoQrCode);
  expect(remoteError({ type: "Zhizi", data: { kind: { type: "InvalidCredentials" }, request_id: "r-42" } }, labels)).toBe("Wrong account or password Request id: r-42");
  expect(remoteError({ type: "Zhizi", data: { kind: { type: "Unknown", data: "odd_key" } } }, labels)).toBe(labels.zhiziErrors.Unknown);
  const profile = (remote: EngineProfile["remote"]): EngineProfile => ({ program: "", model: "", config: "", extra_args: [], remote });
  expect(remoteSummary(profile({ type: "Zhizi", data: { options: { plan: "OnDemand3x", weight: "28bnbt" } } }), labels)).toBe("Zhizi · Pay as you go 3x · 28b nbt");
  expect(remoteSummary(profile({ type: "WebSocket", data: { host: "gpu.example.org", port: 443, secure: true } }), labels)).toBe("Self-hosted · gpu.example.org:443");
  expect(remoteSummary(profile({ type: "Ssh", data: { host: "10.0.0.2", port: 2222, user: "go" } }), labels)).toBe("SSH · go@10.0.0.2:2222");
  expect(remoteSummary(profile(undefined), labels)).toBeNull();
  expect(duration(5400, labels)).toBe("1 h 30 min");
  expect(duration(600, labels)).toBe("10 min");
  expect(yuan("12.50")).toBe("¥12.50");
  expect(yuan(undefined)).toBe("--");
});

describe("ZhiziLogin", () => {
  function setup() {
    const backend = {
      zhiziLoginPassword: vi.fn(() => Promise.resolve({ logged_in: true })),
      zhiziLoginCode: vi.fn(() => Promise.resolve({ logged_in: true })),
      zhiziSendCode: vi.fn(() => Promise.resolve({ retry_after_seconds: 3, may_register_account: true })),
      zhiziResetPassword: vi.fn(() => Promise.resolve({ logged_in: true })),
    };
    const onLoggedIn = vi.fn();
    render(<ZhiziLogin backend={backend} labels={labels} status={{ logged_in: false, store_available: true }} onLoggedIn={onLoggedIn} />);
    return { backend, onLoggedIn };
  }

  test("password login trims the account, never the password", async () => {
    const { backend, onLoggedIn } = setup();
    fireEvent.change(screen.getByTestId("zhizi-account"), { target: { value: " player@example.org " } });
    fireEvent.change(screen.getByTestId("zhizi-password"), { target: { value: " secret " } });
    fireEvent.click(screen.getByTestId("zhizi-remember"));
    await act(async () => fireEvent.click(screen.getByTestId("zhizi-submit")));
    expect(backend.zhiziLoginPassword).toHaveBeenCalledWith("player@example.org", " secret ", true);
    expect(onLoggedIn).toHaveBeenCalledWith({ logged_in: true });
  });

  test("a code waits out its cooldown; a reset checks the new password before sending it", async () => {
    vi.useFakeTimers();
    const { backend } = setup();
    fireEvent.click(screen.getByTestId("zhizi-mode-code"));
    expect(screen.getByText(labels.zhizi.codeCreatesAccount)).toBeTruthy();
    fireEvent.change(screen.getByTestId("zhizi-account"), { target: { value: "13800000000" } });
    await act(async () => fireEvent.click(screen.getByTestId("zhizi-send-code")));
    expect(backend.zhiziSendCode).toHaveBeenCalledWith("13800000000", "FastLogin");
    expect(screen.getByTestId("zhizi-send-code").textContent).toBe("Retry in 3 s");
    // One tick a second, each scheduled after the last renders.
    for (let second = 0; second < 3; second += 1) act(() => vi.advanceTimersByTime(1000));
    expect(screen.getByTestId("zhizi-send-code").textContent).toBe(labels.zhizi.sendCode);

    fireEvent.click(screen.getByTestId("zhizi-forgot"));
    fireEvent.change(screen.getByTestId("zhizi-code"), { target: { value: "123456" } });
    fireEvent.change(screen.getByTestId("zhizi-password"), { target: { value: "short" } });
    await act(async () => fireEvent.click(screen.getByTestId("zhizi-submit")));
    expect(screen.getByTestId("zhizi-error").textContent).toBe(labels.zhizi.passwordTooShort);
    fireEvent.change(screen.getByTestId("zhizi-password"), { target: { value: "long enough" } });
    fireEvent.change(screen.getByTestId("zhizi-confirm"), { target: { value: "long enougH" } });
    await act(async () => fireEvent.click(screen.getByTestId("zhizi-submit")));
    expect(screen.getByTestId("zhizi-error").textContent).toBe(labels.zhizi.passwordMismatch);
    fireEvent.change(screen.getByTestId("zhizi-confirm"), { target: { value: "long enough" } });
    await act(async () => fireEvent.click(screen.getByTestId("zhizi-submit")));
    expect(backend.zhiziResetPassword).toHaveBeenCalledWith("13800000000", "123456", "long enough", false);
  });
});

test("自建算力: a pasted link is tidied by Rust, ws:// is flagged, a QR picture fills it, 测试 shows the version", async () => {
  const backend = {
    normalizeRemoteLink: vi.fn(() => Promise.resolve("ws://192.168.1.8:8080/katago?token=abc")),
    readRemoteLinkQr: vi.fn(() => Promise.resolve("wss://gpu.example.org/k/abc")),
    testRemoteEngine: vi.fn(() => Promise.resolve("1.16.0")),
    pickQrImage: vi.fn(() => Promise.resolve("/Users/me/qr.png")),
  };
  const onEnable = vi.fn(() => Promise.resolve());
  render(<SelfHosted backend={backend} labels={labels} current={null} onEnable={onEnable} />);
  const input = screen.getByTestId("remote-link");
  await act(async () => fireEvent.paste(input, { clipboardData: { getData: () => "“ｗｓ://192.168.1.8:8080/katago?token=abc”" } }));
  expect(backend.normalizeRemoteLink).toHaveBeenCalledWith("“ｗｓ://192.168.1.8:8080/katago?token=abc”");
  expect((input as HTMLInputElement).value).toBe("ws://192.168.1.8:8080/katago?token=abc");
  expect(screen.getByTestId("remote-insecure")).toBeTruthy();

  await act(async () => fireEvent.click(screen.getByTestId("remote-qr")));
  expect(backend.readRemoteLinkQr).toHaveBeenCalledWith("/Users/me/qr.png");
  expect(screen.queryByTestId("remote-insecure")).toBeNull();
  await act(async () => fireEvent.click(screen.getByTestId("remote-test")));
  expect(backend.testRemoteEngine).toHaveBeenCalledWith({ type: "WebSocket", data: { url: "wss://gpu.example.org/k/abc" } });
  expect(screen.getByTestId("remote-version").textContent).toBe("Connected: 1.16.0");
  await act(async () => fireEvent.click(screen.getByTestId("remote-enable")));
  expect(onEnable).toHaveBeenCalledWith("wss://gpu.example.org/k/abc");
});

test("SSH 引擎: the form is checked, a changed host key shows both fingerprints, saving adds the engine", async () => {
  const settings = { version: 1 } as Settings;
  const backend = {
    testRemoteEngine: vi.fn(() => Promise.reject(CHANGED)),
    addRemoteEngine: vi.fn(() => Promise.resolve(settings)),
    updateRemoteEngine: vi.fn(),
    pickKeyFile: vi.fn(() => Promise.resolve("/Users/me/.keys/id_ed25519")),
  };
  const onSaved = vi.fn();
  render(<SshEngineDialog backend={backend} labels={labels} editing={null} onSaved={onSaved} onClose={() => {}} />);
  await act(async () => fireEvent.click(screen.getByTestId("ssh-test")));
  expect(screen.getByTestId("ssh-error").textContent).toBe(labels.ssh.required);
  fireEvent.change(screen.getByTestId("ssh-host"), { target: { value: "gpu.example.org" } });
  fireEvent.change(screen.getByTestId("ssh-user"), { target: { value: "go" } });
  fireEvent.change(screen.getByTestId("ssh-command"), { target: { value: "katago analysis -model m.bin.gz -config a.cfg" } });
  fireEvent.click(screen.getByTestId("ssh-auth-PrivateKey"));
  await act(async () => fireEvent.click(screen.getByTestId("ssh-browse")));
  fireEvent.change(screen.getByTestId("ssh-passphrase"), { target: { value: "phrase" } });
  await act(async () => fireEvent.click(screen.getByTestId("ssh-test")));
  const entry = { type: "Ssh", data: { host: "gpu.example.org", port: 22, user: "go", command: "katago analysis -model m.bin.gz -config a.cfg", auth: { type: "PrivateKey", data: { path: "/Users/me/.keys/id_ed25519", passphrase: "phrase" } } } };
  expect(backend.testRemoteEngine).toHaveBeenCalledWith(entry);
  const warning = screen.getByTestId("ssh-host-key");
  expect(warning.textContent).toContain("SHA256:old");
  expect(warning.textContent).toContain("SHA256:new");
  expect(warning.textContent).toContain("/data/ssh/known_hosts");
  await act(async () => fireEvent.click(screen.getByTestId("ssh-save")));
  expect(backend.addRemoteEngine).toHaveBeenCalledWith("go@gpu.example.org", entry);
  expect(onSaved).toHaveBeenCalledWith(settings);
});

test("启用智子云 adds a 智子 engine with the chosen plan and makes it the main engine", async () => {
  const local: Engine = { id: "local", name: "B18", profile: { program: "/k", model: "/m", config: "", extra_args: [] } };
  const zhizi = { id: "z1", name: "Zhizi cloud", profile: { program: "", model: "", config: "", extra_args: [], remote: { type: "Zhizi", data: { options: { plan: "OnDemand1x", weight: "10b512t" } } } } };
  const backend = {
    zhiziStatus: vi.fn(() => Promise.resolve({ logged_in: true })),
    zhiziOverview: vi.fn(() => Promise.resolve({ account: { phone: "138****0000", membership: false }, balance: { remaining_balance_yuan: "8.00" } })),
    addEngine: vi.fn(() => Promise.resolve({ version: 1, engines: [local, zhizi], main_engine_id: "local" } as unknown as Settings)),
    selectMainEngine: vi.fn(() => Promise.resolve({ version: 1, engines: [local, zhizi], main_engine_id: "z1" } as unknown as Settings)),
  };
  const apply = vi.fn();
  let latest: ReturnType<typeof useRemote> | undefined;
  function Host() {
    const value = useRemote(backend as never, labels, { engines: [local], main: "local", profile: local.profile, apply });
    useEffect(() => {
      latest = value;
    });
    return <>{value.dialog}</>;
  }
  render(<Host />);
  await act(async () => latest!.bindings.remoteCompute!.run!());
  expect(screen.getByTestId("zhizi-balance").textContent).toBe("Balance ¥8.00 · yesterday --");
  fireEvent.change(screen.getByTestId("zhizi-plan"), { target: { value: "OnDemand1x" } });
  await act(async () => fireEvent.click(screen.getByTestId("zhizi-connect")));
  expect(backend.addEngine).toHaveBeenCalledWith("Zhizi cloud", zhizi.profile);
  expect(backend.selectMainEngine).toHaveBeenCalledWith("z1");
  expect(apply).toHaveBeenCalledWith(expect.objectContaining({ main_engine_id: "z1" }));
  expect(latest!.label("B18")).toBe("B18");
});
