import { type FormEvent, useEffect, useState } from "react";
import type { CommentaryDetail, CommentaryKeyStatus, CommentarySettings, CommentaryStyle, CommentaryTerminology, CommentaryVariations } from "../api/commentary";
import type en from "../locales/en.json";

/** Common OpenAI-compatible services; the user can type any other address. */
const PRESETS: readonly { name: string; url: string }[] = [
  { name: "OpenAI", url: "https://api.openai.com/v1" },
  { name: "DeepSeek", url: "https://api.deepseek.com/v1" },
  { name: "Qwen", url: "https://dashscope.aliyuncs.com/compatible-mode/v1" },
  { name: "Kimi", url: "https://api.moonshot.cn/v1" },
  { name: "Ollama", url: "http://localhost:11434/v1" },
];

const RANKS = [...Array.from({ length: 18 }, (_, i) => `${18 - i}k`), ...Array.from({ length: 9 }, (_, i) => `${i + 1}d`)];
const STYLES: readonly CommentaryStyle[] = ["Balanced", "Rigorous", "Patient", "Strict", "Humorous"];
const TERMS: readonly CommentaryTerminology[] = ["Few", "Medium", "Many"];
const DETAILS: readonly CommentaryDetail[] = ["Brief", "Standard", "Detailed"];
const VARIATIONS: readonly CommentaryVariations[] = ["Few", "Moderate", "Many"];

export interface CommentarySetupProps {
  labels: typeof en.commentary;
  settings: Required<CommentarySettings>;
  keyStatus: CommentaryKeyStatus | null;
  /** Saves the settings, then the key when one was typed. */
  onSave(settings: CommentarySettings, key: string): Promise<void>;
  /** Saves the connection the same way, then asks the service for its models. */
  onModels(settings: CommentarySettings, key: string): Promise<string[]>;
  onClearKey(): Promise<void>;
  describe(error: unknown): string;
  onClose(): void;
}

function Choice<T extends string>({ name, value, options, text, onChange }: { name: string; value: T; options: readonly T[]; text: Record<T, string>; onChange(value: T): void }) {
  return (
    <div className="segmented small" role="radiogroup" aria-label={name}>
      {options.map((option) => (
        <button key={option} type="button" role="radio" aria-checked={value === option} data-testid={`commentary-${name}-${option}`} onClick={() => onChange(option)}>
          {text[option]}
        </button>
      ))}
    </div>
  );
}

/** AI 解说设置: the service, its key and model, and how the explanations should read. */
export function CommentarySetup({ labels, settings, keyStatus, onSave, onModels, onClearKey, describe, onClose }: CommentarySetupProps) {
  const text = labels.setup;
  const [form, setForm] = useState(settings);
  const [key, setKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = (patch: Partial<CommentarySettings>) => setForm((current) => ({ ...current, ...patch }));
  const hasKey = keyStatus?.has_key === true;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const missing = (needModel: boolean): string | null => {
    if (form.base_url.trim() === "") return text.enterAddress;
    if (key.trim() === "" && !hasKey) return text.enterKey;
    if (needModel && form.model.trim() === "") return text.enterModel;
    return null;
  };
  const run = (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    work()
      .catch((failure: unknown) => setError(describe(failure)))
      .finally(() => setBusy(false));
  };
  const refresh = () => {
    const problem = missing(false);
    if (problem) return setError(problem);
    setNote(text.loadingModels);
    run(async () => {
      const found = await onModels({ ...form, base_url: form.base_url.trim() }, key.trim());
      setKey("");
      setModels(found);
      setNote(text.modelsLoaded.replace("{count}", String(found.length)));
      if (form.model === "" && found[0]) set({ model: found[0] });
    });
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const problem = missing(true);
    if (problem) return setError(problem);
    run(async () => {
      await onSave({ ...form, base_url: form.base_url.trim(), model: form.model.trim() }, key.trim());
      onClose();
    });
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="dialog commentary-setup" role="dialog" aria-modal="true" aria-labelledby="commentary-setup-title" data-testid="commentary-setup">
        <header className="dialog-header">
          <h2 id="commentary-setup-title">{text.title}</h2>
          <button type="button" className="dialog-close" aria-label={text.cancel} onClick={onClose}>
            ×
          </button>
        </header>
        <form className="commentary-form" noValidate onSubmit={submit}>
          <h3>{text.connection}</h3>
          <div className="field">
            <label htmlFor="commentary-url">{text.baseUrl}</label>
            <input id="commentary-url" className="text-input" data-testid="commentary-url" spellCheck={false} placeholder={text.baseUrlExample} value={form.base_url} onChange={(event) => set({ base_url: event.target.value })} />
            <div className="commentary-presets">
              <span>{text.presets}</span>
              {PRESETS.map((preset) => (
                <button key={preset.name} type="button" className="tag" title={preset.url} onClick={() => set({ base_url: preset.url })}>
                  {preset.name}
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <label htmlFor="commentary-key">{text.apiKey}</label>
            <div className="field-row">
              <input
                id="commentary-key"
                className="text-input"
                data-testid="commentary-key"
                type={showKey ? "text" : "password"}
                autoComplete="off"
                spellCheck={false}
                placeholder={hasKey ? text.keySaved : text.keyPlaceholder}
                value={key}
                onChange={(event) => setKey(event.target.value)}
              />
              <button type="button" className="secondary-button" aria-pressed={showKey} onClick={() => setShowKey((shown) => !shown)}>
                {showKey ? text.hideKey : text.showKey}
              </button>
              {hasKey ? (
                <button type="button" className="secondary-button" data-testid="commentary-clear-key" disabled={busy} onClick={() => run(onClearKey)}>
                  {text.clearKey}
                </button>
              ) : null}
            </div>
            <label className="commentary-check">
              <input type="checkbox" data-testid="commentary-remember" checked={form.remember_key} disabled={keyStatus?.available === false} onChange={(event) => set({ remember_key: event.target.checked })} />
              {text.rememberKey}
            </label>
            <p className="field-path" data-testid="commentary-keychain">
              {keyStatus?.available === false ? text.keychainUnavailable : text.keychainAvailable}
            </p>
          </div>
          <div className="field">
            <label htmlFor="commentary-model">{text.model}</label>
            <div className="field-row">
              <input id="commentary-model" className="text-input" data-testid="commentary-model" list="commentary-models" spellCheck={false} placeholder={text.modelExample} value={form.model} onChange={(event) => set({ model: event.target.value })} />
              <datalist id="commentary-models">
                {models.map((model) => (
                  <option key={model} value={model} />
                ))}
              </datalist>
              <button type="button" className="secondary-button" data-testid="commentary-refresh-models" disabled={busy} onClick={refresh}>
                {text.refreshModels}
              </button>
            </div>
            {note ? <p className="field-path" data-testid="commentary-models-note">{note}</p> : null}
          </div>
          <h3>{text.preferences}</h3>
          <div className="commentary-prefs">
            <label htmlFor="commentary-rank">{text.level}</label>
            <div className="field-row">
              <select id="commentary-rank" data-testid="commentary-rank" value={form.rank} onChange={(event) => set({ rank: event.target.value })}>
                {RANKS.map((rank) => (
                  <option key={rank} value={rank}>
                    {(rank.endsWith("k") ? text.rankKyu : text.rankDan).replace("{n}", rank.slice(0, -1))}
                  </option>
                ))}
              </select>
            </div>
            <label htmlFor="commentary-style">{text.style}</label>
            <div className="field-row">
              <select id="commentary-style" data-testid="commentary-style" value={form.style} onChange={(event) => set({ style: event.target.value as CommentaryStyle })}>
                {STYLES.map((style) => (
                  <option key={style} value={style}>
                    {text.styles[style]}
                  </option>
                ))}
              </select>
            </div>
            <span>{text.terminology}</span>
            <Choice name="terminology" value={form.terminology} options={TERMS} text={text.terms} onChange={(terminology) => set({ terminology })} />
            <span>{text.detail}</span>
            <Choice name="detail" value={form.detail} options={DETAILS} text={text.details} onChange={(detail) => set({ detail })} />
            <span>{text.variations}</span>
            <Choice name="variations" value={form.variations} options={VARIATIONS} text={text.variationLevels} onChange={(variations) => set({ variations })} />
          </div>
          <p className="commentary-privacy" data-testid="commentary-privacy">{text.privacy}</p>
          {error ? (
            <p className="dialog-error" role="alert" data-testid="commentary-setup-error">
              {error}
            </p>
          ) : null}
          <footer className="dialog-footer">
            <button type="button" className="secondary-button" onClick={onClose}>
              {text.cancel}
            </button>
            <button type="submit" className="primary-button" data-testid="commentary-save" disabled={busy}>
              {busy ? text.saving : text.save}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
