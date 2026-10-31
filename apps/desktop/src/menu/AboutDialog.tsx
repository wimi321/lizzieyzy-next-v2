import { useEffect } from "react";
import { version } from "../../package.json";

export interface AboutLabels {
  title: string;
  version: string;
  tagline: string;
  lineage: string;
  license: string;
  source: string;
  close: string;
}

const SOURCE = "github.com/wimi321/lizzieyzy-next-v2";

/** 帮助 → 关于: name, version, lineage and licence. */
export function AboutDialog({ name, labels, onClose }: { name: string; labels: AboutLabels; onClose(): void }) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="dialog-backdrop" role="presentation" onClick={onClose}>
      <section
        className="dialog about-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={labels.title}
        data-testid="about-dialog"
        onClick={(event) => event.stopPropagation()}
      >
        <button type="button" className="dialog-close" aria-label={labels.close} data-testid="about-close" onClick={onClose}>
          ×
        </button>
        <div className="about-mark" aria-hidden="true">
          <span className="about-stone black" />
          <span className="about-stone white" />
        </div>
        <h2 className="about-name">{name}</h2>
        <p className="about-version">{labels.version.replace("{version}", version)}</p>
        <p className="about-tagline">{labels.tagline}</p>
        <p className="about-note">
          {labels.lineage}
          <br />
          {labels.license}
        </p>
        <p className="about-source">
          <span>{labels.source}</span>
          <code>{SOURCE}</code>
        </p>
      </section>
    </div>
  );
}
