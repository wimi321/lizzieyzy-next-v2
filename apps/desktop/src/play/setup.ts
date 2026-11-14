import type { AiPlayer, GameSetup, GameStart, HumanClockView, KataGoPlay, Player, ResignRule } from "../api/game";
import type { GtpTime } from "../api/gtp";
import type { MatchPlayer, MatchSetup } from "../api/match-game";
import type { NewGameOptions } from "../api/generated/NewGameOptions";
import type { PlayLabels } from "./labels";

/** Java's defaults (`Config`): 2 s a move; resign after move 150, 2 in a row under 5%. */
export const KATAGO_DEFAULT: Required<Pick<KataGoPlay, "pure_network">> & KataGoPlay = {
  seconds: 2,
  visits: null,
  pure_network: false,
  resign: { after_moves: 150, consecutive: 2, below_winrate: 0.05 },
  random_opening: null,
};
export const RANDOM_DEFAULT = { within_moves: 20, max_winrate_drop: 0.005, min_visit_share: 0.05 };
/** Java's 引擎对局 resignation: from the start, 2 in a row under 10%. */
export const MATCH_RESIGN: ResignRule = { after_moves: 0, consecutive: 2, below_winrate: 0.1 };
export const NEW_BOARD: NewGameOptions = {
  width: 19,
  height: 19,
  komi: 7.5,
  handicap: 0,
  rules: "chinese",
  black_name: null,
  black_rank: null,
  white_name: null,
  white_rank: null,
};

export type GtpMode = GtpTime["type"];
export const GTP_MODES: readonly GtpMode[] = ["PerMove", "Byoyomi", "Fischer", "Absolute", "EngineDefault", "Custom"];

/** Every GTP time setting's fields at once, so switching modes keeps what was typed. */
export interface GtpTimeForm {
  mode: GtpMode;
  perMove: string;
  main: string;
  period: string;
  stones: string;
  increment: string;
  custom: string;
}

/** The dialog's AI player: KataGo (the main engine) or a GTP engine by id. */
export interface AiForm {
  engine: "katago" | string;
  useSeconds: boolean;
  seconds: string;
  useVisits: boolean;
  visits: string;
  pure: boolean;
  resign: boolean;
  resignAfter: string;
  resignCount: string;
  resignBelow: string;
  random: boolean;
  randomWithin: string;
  randomDrop: string;
  randomShare: string;
  gtp: GtpTimeForm;
}

const text = (value: number | null | undefined, fallback = "") => (value === null || value === undefined ? fallback : String(value));
const percent = (share: number) => String(Math.round(share * 1000) / 10);

/** A whole number in the field, or null when it isn't one (or is out of range). */
export function whole(value: string, min = 0, max = 1_000_000_000): number | null {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const number = Number(trimmed);
  return number >= min && number <= max ? number : null;
}

/** A decimal in the field, or null. */
export function decimal(value: string, min = 0, max = Number.MAX_SAFE_INTEGER): number | null {
  const trimmed = value.trim().replace(",", ".");
  if (!/^-?\d+(\.\d+)?$/.test(trimmed)) return null;
  const number = Number(trimmed);
  return number >= min && number <= max ? number : null;
}

export function gtpTimeForm(time?: GtpTime | null): GtpTimeForm {
  const form: GtpTimeForm = { mode: "PerMove", perMove: "2", main: "10", period: "30", stones: "1", increment: "5", custom: "time_settings 600 30 1" };
  if (!time) return form;
  form.mode = time.type;
  if (time.type === "PerMove") form.perMove = String(time.data);
  if (time.type === "Byoyomi") Object.assign(form, { main: String(time.data.main / 60), period: String(time.data.period), stones: String(time.data.stones) });
  if (time.type === "Fischer") Object.assign(form, { main: String(time.data.main / 60), increment: String(time.data.increment) });
  if (time.type === "Absolute") form.main = String(time.data.main / 60);
  if (time.type === "Custom") form.custom = time.data.join("\n");
  return form;
}

/** Main time is typed in minutes, as Java's dialog does; GTP gets seconds. */
export function gtpTime(form: GtpTimeForm): GtpTime | null {
  const minutes = () => {
    const value = decimal(form.main, 0, 24 * 60);
    return value === null ? null : Math.round(value * 60);
  };
  switch (form.mode) {
    case "PerMove": {
      const seconds = whole(form.perMove, 1, 86_400);
      return seconds === null ? null : { type: "PerMove", data: seconds };
    }
    case "Byoyomi": {
      const main = minutes();
      const period = whole(form.period, 0, 86_400);
      const stones = whole(form.stones, 1, 1000);
      return main === null || period === null || stones === null || main + period === 0 ? null : { type: "Byoyomi", data: { main, period, stones } };
    }
    case "Fischer": {
      const main = minutes();
      const increment = whole(form.increment, 0, 86_400);
      return main === null || increment === null || main + increment === 0 ? null : { type: "Fischer", data: { main, increment } };
    }
    case "Absolute": {
      const main = minutes();
      return main === null || main === 0 ? null : { type: "Absolute", data: { main } };
    }
    case "EngineDefault":
      return { type: "EngineDefault" };
    case "Custom": {
      const lines = form.custom
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line.length > 0);
      return lines.length > 0 ? { type: "Custom", data: lines } : null;
    }
  }
}

export function aiForm(player?: AiPlayer | null): AiForm {
  const katago = player?.type === "KataGo" ? player.data : KATAGO_DEFAULT;
  const resign = katago.resign;
  const random = katago.random_opening;
  return {
    engine: player?.type === "Gtp" ? player.data.engine_id : "katago",
    useSeconds: katago.seconds !== null && katago.seconds !== undefined,
    seconds: text(katago.seconds, "2"),
    useVisits: katago.visits !== null && katago.visits !== undefined,
    visits: text(katago.visits, "800"),
    pure: katago.pure_network ?? false,
    resign: resign !== null && resign !== undefined,
    resignAfter: text(resign?.after_moves, "150"),
    resignCount: text(resign?.consecutive, "2"),
    resignBelow: percent(resign?.below_winrate ?? 0.05),
    random: random !== null && random !== undefined,
    randomWithin: text(random?.within_moves, String(RANDOM_DEFAULT.within_moves)),
    randomDrop: percent(random?.max_winrate_drop ?? RANDOM_DEFAULT.max_winrate_drop),
    randomShare: percent(random?.min_visit_share ?? RANDOM_DEFAULT.min_visit_share),
    gtp: gtpTimeForm(player?.type === "Gtp" ? player.data.time : null),
  };
}

/** The KataGo settings in the form, or null while a field is invalid or no limit is set. */
export function kataGoPlay(form: AiForm): KataGoPlay | null {
  const seconds = form.useSeconds ? decimal(form.seconds, 0.01, 86_400) : null;
  const visits = form.useVisits ? whole(form.visits, 1, 100_000_000) : null;
  if ((form.useSeconds && seconds === null) || (form.useVisits && visits === null)) return null;
  if (!form.pure && seconds === null && visits === null) return null;
  let resign: ResignRule | null = null;
  if (form.resign) {
    const after = whole(form.resignAfter, 0, 10_000);
    const count = whole(form.resignCount, 1, 1000);
    const below = decimal(form.resignBelow, 0, 100);
    if (after === null || count === null || below === null) return null;
    resign = { after_moves: after, consecutive: count, below_winrate: below / 100 };
  }
  let random = null;
  if (form.random) {
    const within = whole(form.randomWithin, 1, 10_000);
    const drop = decimal(form.randomDrop, 0, 100);
    const share = decimal(form.randomShare, 0, 100);
    if (within === null || drop === null || share === null) return null;
    random = { within_moves: within, max_winrate_drop: drop / 100, min_visit_share: share / 100 };
  }
  return { seconds, visits, pure_network: form.pure, resign, random_opening: random };
}

export function aiPlayer(form: AiForm): AiPlayer | null {
  if (form.engine === "katago") {
    const play = kataGoPlay(form);
    return play ? { type: "KataGo", data: play } : null;
  }
  const time = gtpTime(form.gtp);
  return time ? { type: "Gtp", data: { engine_id: form.engine, time } } : null;
}

/** The 人机对局 dialog's own fields; the AI's are an `AiForm`. */
export interface GameForm {
  human: "Black" | "White";
  continueHere: boolean;
  size: number;
  handicap: number;
  free: boolean;
  komi: string;
  rules: string;
  clock: boolean;
  clockMain: string;
  clockSeconds: string;
  clockPeriods: string;
  ponder: boolean;
  autoSave: boolean;
}

function boardOf(start?: GameStart): NewGameOptions {
  if (start?.type === "NewBoard") return start.data;
  if (start?.type === "FreeHandicap") return start.data.options;
  return NEW_BOARD;
}

/** The saved setup as the dialog shows it, with the AI on whichever side it last played. */
export function gameForm(setup?: GameSetup | null): { game: GameForm; ai: AiForm } {
  const black = setup?.black ?? { type: "Human" };
  const white = setup?.white ?? ({ type: "Ai", data: { type: "KataGo", data: KATAGO_DEFAULT } } as Player);
  const human = black.type === "Human" ? "Black" : "White";
  const ai = black.type === "Ai" ? black.data : white.type === "Ai" ? white.data : null;
  const board = boardOf(setup?.start);
  const clock = setup?.human_clock ?? null;
  return {
    game: {
      human,
      continueHere: setup?.start?.type === "CurrentPosition",
      size: board.width,
      handicap: setup?.start?.type === "FreeHandicap" ? setup.start.data.stones : board.handicap,
      free: setup?.start?.type === "FreeHandicap",
      komi: String(board.komi),
      rules: board.rules,
      clock: clock !== null,
      clockMain: String(Math.round((clock?.main_seconds ?? 1200) / 60)),
      clockSeconds: String(clock?.byoyomi_seconds ?? 30),
      clockPeriods: String(clock?.periods ?? 3),
      ponder: setup?.ponder ?? true,
      autoSave: setup?.auto_save ?? true,
    },
    ai: aiForm(ai),
  };
}

/** The setup the dialog describes, or null while a field is invalid. */
export function gameSetup(game: GameForm, ai: AiForm, saved?: GameSetup | null): GameSetup | null {
  const player = aiPlayer(ai);
  if (!player) return null;
  const komi = decimal(game.komi, -1000, 1000);
  if (komi === null) return null;
  let human_clock = null;
  if (game.clock) {
    const main = decimal(game.clockMain, 0, 24 * 60);
    const seconds = whole(game.clockSeconds, 0, 3600);
    const periods = whole(game.clockPeriods, 0, 100);
    if (main === null || seconds === null || periods === null) return null;
    const mainSeconds = Math.round(main * 60);
    if (mainSeconds === 0 && (seconds === 0 || periods === 0)) return null;
    human_clock = { main_seconds: mainSeconds, byoyomi_seconds: seconds, periods };
  }
  const options: NewGameOptions = { ...boardOf(saved?.start), width: game.size, height: game.size, komi, rules: game.rules, handicap: game.free ? 0 : game.handicap };
  const free = game.free && game.handicap >= 2 && player.type === "Gtp";
  const start: GameStart = game.continueHere
    ? { type: "CurrentPosition" }
    : free
      ? { type: "FreeHandicap", data: { options, stones: game.handicap } }
      : { type: "NewBoard", data: options };
  const human: Player = { type: "Human" };
  const machine: Player = { type: "Ai", data: player };
  return {
    black: game.human === "Black" ? human : machine,
    white: game.human === "Black" ? machine : human,
    start,
    human_clock,
    ponder: game.ponder,
    auto_save: game.autoSave,
  };
}

/** One side of 引擎对局: `engine` is "k:<KataGo entry id>" or "g:<GTP entry id>". */
export interface MatchSideForm {
  ai: AiForm;
  wrn: string;
  pda: string;
}

export interface MatchForm {
  games: string;
  swap: boolean;
  size: number;
  handicap: number;
  komi: string;
  rules: string;
  limitMoves: boolean;
  maxMoves: string;
  openings: string[];
  randomOrder: boolean;
  random: boolean;
  randomWithin: string;
  randomDrop: string;
  randomShare: string;
  fromCurrent: boolean;
  autoSave: boolean;
}

export const MATCH_FORM: MatchForm = {
  games: "1",
  swap: true,
  size: 19,
  handicap: 0,
  komi: "7.5",
  rules: "chinese",
  limitMoves: false,
  maxMoves: "450",
  openings: [],
  randomOrder: true,
  random: false,
  randomWithin: "16",
  randomDrop: "0.3",
  randomShare: "5",
  fromCurrent: false,
  autoSave: true,
};

export function matchSideForm(engine: string): MatchSideForm {
  const ai = aiForm({ type: "KataGo", data: { ...KATAGO_DEFAULT, resign: MATCH_RESIGN } });
  return { ai: { ...ai, engine }, wrn: "0", pda: "0" };
}

export function matchPlayer(side: MatchSideForm): MatchPlayer | null {
  const kind = side.ai.engine.slice(0, 2);
  const engine_id = side.ai.engine.slice(2);
  if (!engine_id) return null;
  if (kind === "g:") {
    const time = gtpTime(side.ai.gtp);
    return time ? { type: "Gtp", data: { engine_id, time } } : null;
  }
  const play = kataGoPlay(side.ai);
  const wrn = decimal(side.wrn, 0, 5);
  const pda = decimal(side.pda, -3, 3);
  if (kind !== "k:" || !play || wrn === null || pda === null) return null;
  return { type: "KataGo", data: { engine_id, play, wrn, pda } };
}

export function matchSetup(form: MatchForm, a: MatchSideForm, b: MatchSideForm): MatchSetup | null {
  const playerA = matchPlayer(a);
  const playerB = matchPlayer(b);
  const games = whole(form.games, 1, 10_000);
  const komi = decimal(form.komi, -1000, 1000);
  const maxMoves = form.limitMoves ? whole(form.maxMoves, 1, 10_000) : null;
  if (!playerA || !playerB || games === null || komi === null || (form.limitMoves && maxMoves === null)) return null;
  let random_opening = null;
  if (form.random) {
    const within = whole(form.randomWithin, 1, 10_000);
    const drop = decimal(form.randomDrop, 0, 100);
    const share = decimal(form.randomShare, 0, 100);
    if (within === null || drop === null || share === null) return null;
    random_opening = { within_moves: within, max_winrate_drop: drop / 100, min_visit_share: share / 100 };
  }
  const options: NewGameOptions = { ...NEW_BOARD, width: form.size, height: form.size, komi, handicap: form.handicap, rules: form.rules };
  return {
    a: playerA,
    b: playerB,
    games,
    swap_colors: form.swap,
    start: form.fromCurrent ? { type: "CurrentPosition" } : { type: "NewBoard", data: options },
    max_moves: maxMoves,
    openings: form.openings,
    random_openings: form.randomOrder,
    random_opening,
    auto_save: form.autoSave,
  };
}

/** "B+R", "W+T", "B+3.5", "0" in words. */
export function resultText(result: string | null | undefined, labels: PlayLabels): string | null {
  if (!result) return null;
  if (result === "0" || /^draw$/i.test(result)) return labels.draw;
  const match = /^([BW])\+(.*)$/.exec(result);
  if (!match) return result;
  const winner = match[1] === "B" ? labels.blackWins : labels.whiteWins;
  const how = match[2] ?? "";
  if (/^R(esign)?$/i.test(how)) return `${winner} · ${labels.reasons.Resignation}`;
  if (/^T(ime)?$/i.test(how)) return `${winner} · ${labels.reasons.Timeout}`;
  return how ? `${winner} ${labels.byPoints.replace("{n}", how)}` : winner;
}

const pad = (value: number) => String(Math.floor(value)).padStart(2, "0");

/** Main time as m:ss, then byo-yomi as its seconds and periods left. */
export function clockText(clock: HumanClockView | null | undefined, labels: PlayLabels): string | null {
  if (!clock) return null;
  const main = Math.max(0, Math.ceil(clock.main_seconds ?? 0));
  if (main > 0) return `${Math.floor(main / 60)}:${pad(main % 60)}`;
  return labels.byoyomiLeft.replace("{seconds}", String(Math.max(0, Math.ceil(clock.byoyomi_seconds ?? 0)))).replace("{periods}", String(clock.periods ?? 0));
}

/** Under ten seconds of the last time the player has. */
export function clockUrgent(clock: HumanClockView | null | undefined): boolean {
  if (!clock) return false;
  const main = clock.main_seconds ?? 0;
  return main <= 0 ? (clock.byoyomi_seconds ?? 0) <= 10 : main <= 10 && (clock.periods ?? 0) === 0;
}

/** Splits a command line as a shell would for plain words and quoted values. */
export function splitArgs(line: string): string[] {
  const args: string[] = [];
  let current = "";
  let quote: '"' | "'" | null = null;
  let started = false;
  for (const char of line) {
    if (quote) {
      if (char === quote) quote = null;
      else current += char;
    } else if (char === '"' || char === "'") {
      quote = char;
      started = true;
    } else if (/\s/.test(char)) {
      if (started) args.push(current);
      current = "";
      started = false;
    } else {
      current += char;
      started = true;
    }
  }
  if (started) args.push(current);
  return args;
}

export function joinArgs(args: readonly string[]): string {
  return args.map((arg) => (arg === "" || /[\s"']/.test(arg) ? `"${arg.replace(/"/g, "'")}"` : arg)).join(" ");
}
