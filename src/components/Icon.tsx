import type { SVGProps } from "react";

export type IconName =
  | "search"
  | "settings"
  | "close"
  | "folder"
  | "trash"
  | "plus"
  | "clipboard"
  | "image"
  | "sparkles"
  | "database"
  | "keyboard"
  | "check"
  | "alert"
  | "pause"
  | "play"
  | "eye"
  | "checkSquare"
  | "type"
  | "code"
  | "clock";

const paths: Record<IconName, React.ReactNode> = {
  search: <><circle cx="11" cy="11" r="6.5" /><path d="m16 16 4.5 4.5" /></>,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.86 2.86-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21h-4v-.1A1.7 1.7 0 0 0 8.6 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06-2.86-2.86.06-.06A1.7 1.7 0 0 0 4.2 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H2.4v-4h.1A1.7 1.7 0 0 0 4.2 8.6a1.7 1.7 0 0 0-.34-1.88l-.06-.06L6.66 3.8l.06.06A1.7 1.7 0 0 0 8.6 4.2a1.7 1.7 0 0 0 1-.6A1.7 1.7 0 0 0 10 2.5v-.1h4v.1a1.7 1.7 0 0 0 1 1.7 1.7 1.7 0 0 0 1.88-.34l.06-.06 2.86 2.86-.06.06a1.7 1.7 0 0 0-.34 1.88 1.7 1.7 0 0 0 .6 1 1.7 1.7 0 0 0 1.1.4h.1v4h-.1a1.7 1.7 0 0 0-1.7 1Z" /></>,
  close: <path d="m6 6 12 12M18 6 6 18" />,
  folder: <path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-10Z" />,
  trash: <><path d="M4.5 7h15M9 7V4.5h6V7m2.5 0-.7 13h-9.6L6.5 7M10 11v5.5M14 11v5.5" /></>,
  plus: <path d="M12 5v14M5 12h14" />,
  clipboard: <><rect x="5" y="5" width="14" height="16" rx="3" /><path d="M9 5.5V4a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v1.5M9 11h6M9 15h6" /></>,
  image: <><rect x="3" y="4" width="18" height="16" rx="3" /><circle cx="8.5" cy="9" r="1.5" /><path d="m4 17 4.5-4 3.5 3 3-2.5 5 4.5" /></>,
  sparkles: <><path d="m12 3 .9 2.6L15.5 7l-2.6 1.4L12 11l-.9-2.6L8.5 7l2.6-1.4L12 3ZM6 13l.7 2L9 16.2l-2.3 1.2L6 19.5l-.7-2.1L3 16.2 5.3 15 6 13ZM18 12l.7 2 2.3 1.2-2.3 1.2-.7 2.1-.7-2.1-2.3-1.2 2.3-1.2.7-2Z" /></>,
  database: <><ellipse cx="12" cy="5.5" rx="7.5" ry="3" /><path d="M4.5 5.5v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6M4.5 11.5v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6" /></>,
  keyboard: <><rect x="2.5" y="5" width="19" height="14" rx="3" /><path d="M6 9h1M10 9h1M14 9h1M18 9h.1M6 13h1M10 13h1M14 13h4M7 16h10" /></>,
  check: <path d="m5 12.5 4.5 4.5L19 7.5" />,
  alert: <><path d="M10.2 4.2 2.9 17a2 2 0 0 0 1.75 3h14.7a2 2 0 0 0 1.75-3L13.8 4.2a2 2 0 0 0-3.6 0Z" /><path d="M12 9v4M12 17h.01" /></>,
  pause: <><path d="M9 7v10M15 7v10" /></>,
  play: <path d="m9 7 8 5-8 5V7Z" />,
  eye: <><path d="M2.5 12S6 6.5 12 6.5 21.5 12 21.5 12 18 17.5 12 17.5 2.5 12 2.5 12Z" /><circle cx="12" cy="12" r="2.8" /></>,
  checkSquare: <><rect x="3.5" y="3.5" width="17" height="17" rx="5" /><path d="m8.5 12.2 2.6 2.6L16 9.5" /></>,
  type: <path d="M5 7V4.5h14V7M12 4.5v15M9.5 19.5h5" />,
  code: <><path d="m8.5 8-4 4 4 4" /><path d="m15.5 8 4 4-4 4" /></>,
  clock: <><circle cx="12" cy="12" r="8.5" /><path d="M12 7.5V12l3 2" /></>,
};

export function Icon({ name, ...props }: { name: IconName } & SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>
      {paths[name]}
    </svg>
  );
}
