/** 16px stroke icons drawn in currentColor, so they follow the theme. */
type IconName = "open" | "save" | "first" | "previous" | "next" | "last" | "pause" | "resume";

const PATHS: Record<IconName, string> = {
  open: "M2 4.5h4l1.5 1.5H14v6.5a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z",
  save: "M8 2.5v7m0 0L5 6.5m3 3 3-3M3 11v1.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V11",
  first: "M4 3.5v9M12.5 3.5 7 8l5.5 4.5z",
  previous: "M11 3.5 5 8l6 4.5z",
  next: "M5 3.5 11 8l-6 4.5z",
  last: "M12 3.5v9M3.5 3.5 9 8l-5.5 4.5z",
  pause: "M5.5 3.5v9M10.5 3.5v9",
  resume: "M5 3.5 12 8l-7 4.5z",
};

export function Icon({ name }: { name: IconName }) {
  return (
    <svg className="icon" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <path
        d={PATHS[name]}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
