import { type FormEvent, useEffect, useState } from "react";
import type { Backend } from "@/api/backend";
import type { ZhiziStatus, ZhiziVerificationPurpose } from "../api/zhizi";
import type en from "../locales/en.json";
import { remoteError } from "./format";

type LoginBackend = Pick<Backend, "zhiziLoginPassword" | "zhiziLoginCode" | "zhiziSendCode" | "zhiziResetPassword">;
type Mode = "password" | "code" | "reset";

export interface ZhiziLoginProps {
  backend: LoginBackend;
  labels: typeof en.remote;
  status: ZhiziStatus | null;
  onLoggedIn(status: ZhiziStatus): void;
}

/** Java's 智子 login: password or code login (which can register), and 忘记密码. No password is kept. */
export function ZhiziLogin({ backend, labels, status, onLoggedIn }: ZhiziLoginProps) {
  const text = labels.zhizi;
  const [mode, setMode] = useState<Mode>("password");
  const [account, setAccount] = useState("");
  const [password, setPassword] = useState("");
  const [shown, setShown] = useState(false);
  const [code, setCode] = useState("");
  const [confirm, setConfirm] = useState("");
  const [remember, setRemember] = useState(false);
  const [cooldown, setCooldown] = useState(0);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const storeAvailable = status?.store_available !== false;

  useEffect(() => {
    if (cooldown <= 0) return;
    const timer = window.setTimeout(() => setCooldown((left) => left - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [cooldown]);

  const run = (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    work()
      .catch((failure: unknown) => setError(remoteError(failure, labels)))
      .finally(() => setBusy(false));
  };
  // Accounts are trimmed; passwords never are (Rust follows the same rule).
  const who = account.trim();

  const sendCode = (purpose: ZhiziVerificationPurpose) => {
    if (!who) return setError(text.accountRequired);
    run(async () => {
      const delivery = await backend.zhiziSendCode!(who, purpose);
      setCooldown(delivery.retry_after_seconds ?? 60);
      setNote(text.codeSent);
    });
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!who) return setError(text.accountRequired);
    if (mode === "password") {
      if (!password) return setError(text.passwordRequired);
      return run(async () => onLoggedIn(await backend.zhiziLoginPassword!(who, password, remember)));
    }
    if (!code.trim()) return setError(text.codeRequired);
    if (mode === "code") return run(async () => onLoggedIn(await backend.zhiziLoginCode!(who, code.trim(), remember)));
    if (password.length < 8) return setError(text.passwordTooShort);
    if (password !== confirm) return setError(text.passwordMismatch);
    run(async () => onLoggedIn(await backend.zhiziResetPassword!(who, code.trim(), password, remember)));
  };

  const switchTo = (next: Mode) => {
    setMode(next);
    setError(null);
    setNote(null);
    setPassword("");
    setConfirm("");
  };

  return (
    <form className="zhizi-login" noValidate onSubmit={submit} data-testid="zhizi-login">
      {mode === "reset" ? (
        <h3>{text.resetTitle}</h3>
      ) : (
        <div className="segmented small" role="radiogroup" aria-label={text.login}>
          {(["password", "code"] as const).map((value) => (
            <button key={value} type="button" role="radio" aria-checked={mode === value} data-testid={`zhizi-mode-${value}`} onClick={() => switchTo(value)}>
              {value === "password" ? text.passwordLogin : text.codeLogin}
            </button>
          ))}
        </div>
      )}
      <label className="remote-field">
        <span>{text.account}</span>
        <input className="text-input" data-testid="zhizi-account" autoComplete="username" placeholder={text.accountPlaceholder} value={account} onChange={(event) => setAccount(event.target.value)} />
      </label>
      {mode !== "password" ? (
        <label className="remote-field">
          <span>{text.code}</span>
          <span className="remote-row">
            <input className="text-input" data-testid="zhizi-code" inputMode="numeric" autoComplete="one-time-code" placeholder={text.codePlaceholder} value={code} onChange={(event) => setCode(event.target.value)} />
            <button type="button" className="secondary-button" data-testid="zhizi-send-code" disabled={busy || cooldown > 0} onClick={() => sendCode(mode === "reset" ? "ResetPassword" : "FastLogin")}>
              {cooldown > 0 ? text.sendCodeCountdown.replace("{n}", String(cooldown)) : text.sendCode}
            </button>
          </span>
        </label>
      ) : null}
      {mode !== "code" ? (
        <label className="remote-field">
          <span>{mode === "reset" ? text.newPassword : text.password}</span>
          <span className="remote-row">
            <input
              className="text-input"
              data-testid="zhizi-password"
              type={shown ? "text" : "password"}
              autoComplete={mode === "reset" ? "new-password" : "current-password"}
              placeholder={mode === "reset" ? text.newPasswordPlaceholder : text.passwordPlaceholder}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
            />
            <button type="button" className="secondary-button" aria-pressed={shown} onClick={() => setShown((value) => !value)}>
              {shown ? text.hidePassword : text.showPassword}
            </button>
          </span>
        </label>
      ) : null}
      {mode === "reset" ? (
        <label className="remote-field">
          <span>{text.confirmPassword}</span>
          <input className="text-input" data-testid="zhizi-confirm" type={shown ? "text" : "password"} autoComplete="new-password" value={confirm} onChange={(event) => setConfirm(event.target.value)} />
        </label>
      ) : null}
      <label className="commentary-check remote-check">
        <input type="checkbox" data-testid="zhizi-remember" checked={remember} disabled={!storeAvailable} onChange={(event) => setRemember(event.target.checked)} />
        {text.rememberLogin}
      </label>
      <p className="remote-hint">{storeAvailable ? text.rememberHint : text.storeUnavailable}</p>
      {mode !== "password" ? <p className="remote-hint">{text.codeCreatesAccount}</p> : null}
      {note && !error ? <p className="remote-note" data-testid="zhizi-note">{note}</p> : null}
      {error ? (
        <p className="dialog-error" role="alert" data-testid="zhizi-error">
          {error}
        </p>
      ) : null}
      <div className="remote-actions">
        {mode === "reset" ? (
          <button type="button" className="link-button" onClick={() => switchTo("password")}>
            {text.backToLogin}
          </button>
        ) : (
          <button type="button" className="link-button" data-testid="zhizi-forgot" onClick={() => switchTo("reset")}>
            {text.forgotPassword}
          </button>
        )}
        <button type="submit" className="primary-button" data-testid="zhizi-submit" disabled={busy}>
          {mode === "reset" ? text.resetAndLogin : text.login}
        </button>
      </div>
    </form>
  );
}
