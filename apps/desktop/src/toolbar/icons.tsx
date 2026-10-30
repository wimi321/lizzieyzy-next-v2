/** 16px stroke icons drawn in currentColor, so they follow the theme. */
export type IconName = "open" | "save" | "first" | "previous" | "next" | "last" | "pause" | "resume" | "coordinates" | "territory" | "settings" | "bolt" | "fox";

const PATHS: Record<IconName, string> = {
  open: "M2 4.5h4l1.5 1.5H14v6.5a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z",
  save: "M8 2.5v7m0 0L5 6.5m3 3 3-3M3 11v1.5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V11",
  first: "M4 3.5v9M12.5 3.5 7 8l5.5 4.5z",
  previous: "M11 3.5 5 8l6 4.5z",
  next: "M5 3.5 11 8l-6 4.5z",
  last: "M12 3.5v9M3.5 3.5 9 8l-5.5 4.5z",
  pause: "M5.5 3.5v9M10.5 3.5v9",
  resume: "M5 3.5 12 8l-7 4.5z",
  coordinates: "M2.5 5.5h11M2.5 10.5h11M5.5 2.5v11M10.5 2.5v11",
  territory: "M2.5 4.5 6 3l4 1.5 3.5-1.5v8.5L10 13l-4-1.5-3.5 1.5zM6 3v8.5M10 4.5V13",
  fox: "M2.5 2.5 6 5.5h4l3.5-3v5L8 13.5 2.5 7.5zM6 8.5h.01M10 8.5h.01",
  bolt: "M8.9 1.75 3.75 9h4.1l-.75 5.25L12.25 7H8.2z",
  settings:
    "M8 5.75a2.25 2.25 0 1 0 0 4.5 2.25 2.25 0 1 0 0-4.5zM8 1.75v1.5M8 12.75v1.5M1.75 8h1.5M12.75 8h1.5M3.6 3.6l1.05 1.05M11.35 11.35l1.05 1.05M3.6 12.4l1.05-1.05M11.35 4.65l1.05-1.05",
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
