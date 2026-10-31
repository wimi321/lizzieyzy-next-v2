import type {
  AnalysisView,
  AppEvent,
  BoardView,
  CandidateView,
  ColorView,
  DocumentView,
  GraphView,
  FoxSearchResult,
  MoveView,
  PhaseReport,
  PointView,
  VariationPreviewView,
  StrengthEstimateView,
  SideReport,
} from "../src/api/backend";

type Stone = [x: number, y: number, color: ColorView];

/** Scripted Fox reply for UI tests; network parsing remains in Rust. */
export function foxSearchResult(): FoxSearchResult {
  return {
    account: { uid: "123456", nickname: "测试棋手" },
    page: {
      games: [{
        id: "202610300001",
        start_time: "2026-10-30 12:00:00",
        black: { uid: "123456", nickname: "测试棋手", rank: "9段" },
        white: { uid: "654321", nickname: "对手", rank: "P1段" },
        result: { winner: "Black", kind: "Resignation", score: null, unit: null },
        move_count: 153,
      }],
      next_cursor: "202610300001",
    },
  };
}

function phase(samples: number, scoreLoss: number | null, firstChoice: number): PhaseReport {
  return {
    sample_count: samples,
    average_winrate_loss: (scoreLoss ?? 0) * 2.4,
    average_score_loss: scoreLoss,
    weighted_score_loss: (scoreLoss ?? 0) * 1.3,
    good_move_rate: Math.min(1, firstChoice + 0.25),
    first_choice_rate: firstChoice,
  };
}

/** A side as Rust reports it after a whole-game analysis. Values are plausible, not computed. */
export function strengthSide(options: {
  rank: number;
  zh: string;
  en: string;
  band: string;
  match: number;
  first: number;
  tiers: [number, number, number, number, number, number];
  phases: [PhaseReport, PhaseReport, PhaseReport];
}): SideReport {
  const samples = options.tiers.reduce((sum, count) => sum + count, 0);
  return {
    sample_count: samples,
    score_sample_count: samples,
    rank_value: options.rank,
    rank_label_zh: options.zh,
    rank_label_en: options.en,
    prediction_sigma: 0.9,
    quality_score: ((options.rank + 18) * 100) / 30,
    average_winrate_loss: 2.1,
    average_score_loss: 0.84,
    median_score_loss: 0.4,
    weighted_score_loss: 1.12,
    average_score_equivalent_loss: 0.84,
    match_rate: options.match,
    first_choice_rate: options.first,
    top3_rate: Math.min(1, options.first + 0.3),
    top5_rate: Math.min(1, options.first + 0.38),
    good_move_rate: Math.min(1, options.first + 0.24),
    move_rank_good_move_rate: Math.min(1, options.first + 0.24),
    move_rank_counts: options.tiers,
    tier_counts: options.tiers,
    bad_move_rate: (options.tiers[3] + options.tiers[4] + options.tiers[5]) / samples,
    mistake_rate: (options.tiers[4] + options.tiers[5]) / samples,
    blunder_rate: options.tiers[5] / samples,
    average_difficulty: 0.42,
    strength_band: options.band,
    confidence: samples >= 40 ? "High" : samples >= 16 ? "Medium" : "Low",
    ai_likelihood: 0.05,
    features: Array.from({ length: 29 }, () => 0) as SideReport["features"],
    opening: options.phases[0],
    middlegame: options.phases[1],
    endgame: options.phases[2],
    samples: [],
  };
}

/** A finished 211-move game: Black a little stronger, both well analysed. */
export function strengthView(revision: number, analyzed = 211): StrengthEstimateView {
  const black = strengthSide({
    rank: 5.3, zh: "业余5.3段", en: "5.3 dan", band: "5-6d", match: 0.74, first: 0.48,
    tiers: [51, 27, 14, 7, 5, 2],
    phases: [phase(30, 0.42, 0.57), phase(50, 1.08, 0.42), phase(26, 0.71, 0.5)],
  });
  const white = strengthSide({
    rank: 3.8, zh: "业余3.8段", en: "3.8 dan", band: "3-4d", match: 0.66, first: 0.41,
    tiers: [43, 25, 17, 9, 6, 5],
    phases: [phase(30, 0.55, 0.5), phase(50, 1.31, 0.36), phase(25, 0.62, 0.44)],
  });
  return {
    revision,
    report: { black, white, overall: black, has_enough_data: true },
    total_moves: 211,
    analyzed_moves: analyzed,
  };
}

export function boardView(
  width: number,
  height: number,
  stones: Stone[],
  options: { toPlay?: ColorView; moveNumber?: number; last?: Stone } = {},
): BoardView {
  const cells: (ColorView | null)[] = Array.from({ length: width * height }, () => null);
  for (const [x, y, color] of stones) cells[y * width + x] = color;
  const last = options.last;
  return {
    width,
    height,
    stones: cells,
    last_move: last ? { type: "Play", data: { color: last[2], point: { x: last[0], y: last[1] } } } : null,
    to_play: options.toPlay ?? "Black",
    move_number: options.moveNumber ?? stones.length,
    captured_black: 0,
    captured_white: 0,
  };
}

/** A displayed line of `count` points whose node ids equal their move numbers. */
export function graphView(count: number, current: number, summaries: Record<number, [winrate: number, score: number]>): GraphView {
  return {
    points: Array.from({ length: count }, (_, index) => {
      const summary = summaries[index];
      return {
        node_id: index,
        move_number: index,
        summary: summary ? { winrate: summary[0], score_lead: summary[1], visits: 400 + index * 10 } : null,
      };
    }),
    current_index: current,
  };
}

export function documentView(board: BoardView, current: number, graph?: GraphView): DocumentView {
  return {
    board,
    tree: { root: 0, current, main_line: Array.from({ length: current + 1 }, (_, i) => i), variation_count: 1, structure_revision: 0 },
    game_info: {
      black_name: null, black_rank: null, white_name: null, white_rank: null,
      komi: 7.5, rules: null, handicap: null, result: null, date: null, event: null,
    },
    graph: graph ?? graphView(current + 1, current, {}),
    comment: null,
    warnings: [],
    position_error: null,
  };
}

/** A smooth, deterministic game: Black leads early, White turns it around late. */
export function gameSummaries(analysed: number): Record<number, [number, number]> {
  const summaries: Record<number, [number, number]> = {};
  for (let index = 0; index < analysed; index++) {
    const swing = Math.sin(index / 9) * 0.12 + Math.sin(index / 3.1) * 0.03;
    const trend = index < 60 ? 0.04 : -((index - 60) / 30) * 0.3;
    const winrate = Math.min(0.97, Math.max(0.03, 0.5 + swing + trend));
    summaries[index] = [Math.round(winrate * 1000) / 1000, Math.round((winrate - 0.5) * 300) / 10];
  }
  return summaries;
}

export function documentChanged(view: DocumentView, revision: number): AppEvent {
  return { type: "DocumentChanged", data: { revision, view } };
}

export function analysisUpdated(view: AnalysisView): AppEvent {
  return { type: "AnalysisUpdated", data: view };
}

const LETTERS = "ABCDEFGHJKLMNOPQRST";

function gtp(point: PointView, rows: number): string {
  return `${LETTERS[point.x]}${rows - point.y}`;
}

/** Colors alternate from the side to move, as app-core sends them. */
function variation(points: PointView[], toPlay: ColorView): MoveView[] {
  return points.map((point, index) => ({
    type: "Play",
    data: { color: (index % 2 === 0) === (toPlay === "Black") ? "Black" : "White", point },
  }));
}

export function candidate(
  point: PointView,
  order: number,
  winrate: number,
  visits: number,
  scoreLead: number,
  pv: PointView[],
  board: BoardView,
): CandidateView {
  return {
    coordinate: gtp(point, board.height),
    point,
    visits,
    winrate,
    score_lead: scoreLead,
    prior: 0.1,
    order,
    pv: variation([point, ...pv], board.to_play),
  };
}

export function analysisView(
  revision: number,
  nodeId: number,
  candidates: CandidateView[],
  options: { visits?: number; visits_per_second?: number | null; ownership?: number[] | null } = {},
): AnalysisView {
  const best = candidates[0];
  const winrate = best?.winrate ?? 0.5;
  const scoreLead = best?.score_lead ?? 0;
  const visits = options.visits ?? candidates.reduce((sum, c) => sum + c.visits, 0);
  return {
    node_id: nodeId,
    revision,
    winrate,
    score_lead: scoreLead,
    visits,
    summary: { winrate, score_lead: scoreLead, visits },
    visits_per_second: options.visits_per_second === undefined ? 1200 : options.visits_per_second,
    candidates,
    ownership: options.ownership ?? null,
    is_during_search: true,
    source: "Live",
  };
}

/** A deterministic middle-game position; White just played, so Black is to move. */
export const MID_GAME_STONES: Stone[] = [
  [3, 3, "Black"], [15, 3, "White"], [3, 15, "Black"], [15, 15, "White"], [2, 13, "White"],
  [5, 16, "Black"], [16, 13, "Black"], [13, 16, "White"], [9, 3, "Black"], [13, 2, "White"],
  [16, 5, "White"], [2, 5, "Black"], [4, 2, "White"], [5, 3, "Black"], [16, 10, "Black"],
  [3, 10, "White"], [9, 15, "Black"], [11, 16, "White"], [14, 9, "Black"], [9, 9, "White"],
];

export function midGame(graph?: GraphView) {
  return documentView(boardView(19, 19, MID_GAME_STONES, { toPlay: "Black", moveNumber: 20, last: [9, 9, "White"] }), 20, graph);
}

/**
 * A mid-game candidate at D16 whose variation surrounds White's E17 stone,
 * and Rust's replay of it: E17 is captured, the seven variation stones are numbered.
 */
export function captureVariation(revision: number) {
  const board = midGame().board!;
  const line: PointView[] = [{ x: 10, y: 6 }, { x: 3, y: 2 }, { x: 12, y: 5 }, { x: 5, y: 2 }, { x: 11, y: 8 }, { x: 4, y: 1 }];
  const capture = candidate({ x: 4, y: 3 }, 0, 0.58, 2100, 2.2, line, board);
  const analysis = midGameAnalysis(revision, 20);
  analysis.candidates = [capture, ...analysis.candidates.map((c) => ({ ...c, order: c.order + 1 }))];
  const played: Stone[] = capture.pv.map((move) => {
    if (move.type !== "Play") throw new Error("the fixture variation has no passes");
    return [move.data.point.x, move.data.point.y, move.data.color];
  });
  const survivors = MID_GAME_STONES.filter(([x, y]) => !(x === 4 && y === 2));
  const last = played[played.length - 1]!;
  return {
    analysis,
    moves: capture.pv,
    replay: {
      revision,
      node_id: 20,
      board: boardView(19, 19, [...survivors, ...played], { toPlay: "White", moveNumber: 27, last }),
      applied_moves: capture.pv,
      stones: played.map(([x, y, color], index) => ({ point: { x, y }, color, move_number: index + 1 })),
      stop_reason: null,
    } satisfies VariationPreviewView,
  };
}

/** The middle game on a 121-move line whose first 91 moves have been analysed. */
export function analysedGame() {
  return midGame(graphView(121, 20, gameSummaries(91)));
}

export function midGameAnalysis(revision: number, nodeId: number, visitsScale = 1, withOwnership = false) {
  const board = midGame().board!;
  const scale = (n: number) => Math.round(n * visitsScale);
  const candidates = [
    candidate({ x: 10, y: 6 }, 0, 0.562, scale(1840), 1.6, [{ x: 12, y: 5 }, { x: 11, y: 8 }, { x: 13, y: 7 }, { x: 6, y: 9 }], board),
    candidate({ x: 6, y: 9 }, 1, 0.553, scale(910), 1.2, [{ x: 10, y: 6 }, { x: 7, y: 12 }], board),
    candidate({ x: 13, y: 12 }, 2, 0.531, scale(420), 0.5, [{ x: 12, y: 13 }, { x: 10, y: 6 }], board),
    candidate({ x: 2, y: 8 }, 3, 0.502, scale(260), -0.4, [{ x: 4, y: 8 }], board),
    candidate({ x: 17, y: 2 }, 4, 0.47, scale(120), -1.3, [{ x: 16, y: 2 }], board),
    candidate({ x: 6, y: 6 }, 5, 0.41, scale(60), -3.2, [{ x: 7, y: 7 }], board),
  ];
  return analysisView(revision, nodeId, candidates, {
    ownership: withOwnership ? ownershipPattern(19, 19) : null,
  });
}

/** Smooth, deterministic territory: Black on the left, White on the right. */
export function ownershipPattern(width: number, height: number): number[] {
  return Array.from({ length: width * height }, (_, index) => {
    const x = index % width;
    const y = Math.floor(index / width);
    const lean = (width - 1 - 2 * x) / (width - 1);
    const wobble = Math.sin((y + 1) * 0.7) * 0.25;
    return Math.max(-1, Math.min(1, lean * 1.2 + wobble * (1 - Math.abs(lean))));
  });
}

export function nineByNine() {
  const stones: Stone[] = [
    [2, 2, "Black"], [6, 6, "White"], [6, 2, "Black"], [2, 6, "White"], [4, 4, "Black"], [5, 3, "White"],
  ];
  return documentView(boardView(9, 9, stones, { toPlay: "Black", moveNumber: 6, last: [5, 3, "White"] }), 6);
}

export function emptyNineteen() {
  return documentView(boardView(19, 19, [], { moveNumber: 0 }), 0);
}

/** Scripted empty backend result; fixtures do not run the strength estimator. */
export function emptyStrengthEstimate(revision = 0): StrengthEstimateView {
  const phase = {
    sample_count: 0, average_winrate_loss: 0, average_score_loss: null,
    weighted_score_loss: 0, good_move_rate: 0, first_choice_rate: 0,
  };
  const side: SideReport = {
    sample_count: 0, score_sample_count: 0, rank_value: null, prediction_sigma: null,
    rank_label_zh: "-", rank_label_en: "-",
    quality_score: 0, average_winrate_loss: 0, average_score_loss: null,
    median_score_loss: null, weighted_score_loss: 0, average_score_equivalent_loss: 0,
    match_rate: 0, first_choice_rate: 0, top3_rate: 0, top5_rate: 0, good_move_rate: 0,
    move_rank_good_move_rate: 0, move_rank_counts: [0, 0, 0, 0, 0, 0],
    tier_counts: [0, 0, 0, 0, 0, 0], bad_move_rate: 0, mistake_rate: 0,
    blunder_rate: 0, average_difficulty: 0, strength_band: "-", confidence: "Low",
    ai_likelihood: 0,
    features: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    opening: { ...phase }, middlegame: { ...phase }, endgame: { ...phase }, samples: [],
  };
  return {
    revision, total_moves: 0, analyzed_moves: 0,
    report: {
      black: structuredClone(side), white: structuredClone(side), overall: structuredClone(side),
      has_enough_data: false,
    },
  };
}
