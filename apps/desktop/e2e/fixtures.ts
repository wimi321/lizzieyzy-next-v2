import type {
  EngineProfile,
  CommentarySettings,
  CommentaryUpdated,
  CommentaryKeyStatus,
  AnalysisView,
  GameSetup,
  GameView,
  CoachSetup,
  CoachView,
  CoachMoveRated,
  CoachSummary,
  MatchView,
  GtpConsoleLine,
  BatchAnalysisStatus,
  FlashAnalysisSettings,
  AppEvent,
  BoardView,
  CandidateView,
  ColorView,
  DocumentView,
  GraphView,
  FoxSearchResult,
  TencentPage,
  MoveView,
  PhaseReport,
  PointView,
  ProblemMovesView,
  Sample,
  VariationNodeView,
  VariationPreviewView,
  VariationTreeView,
  StrengthEstimateView,
  StrengthPhasesReport,
  SideReport,
  UpdateCheck,
  UpdateDownloadStatus,
  FullLogStatus,
} from "../src/api/backend";

type Stone = [x: number, y: number, color: ColorView];

/** Optional remote settings are fixture data; connection logic stays in Rust. */
export function fillRemoteProfileDefaults(profile: EngineProfile): void {
  gameDefaultField(profile, "remote", undefined);
  const remote = profile.remote;
  if (!remote) return;
  gameDefaultField(remote.data, "host", "");
  gameDefaultField(remote.data, "port", remote.type === "Ssh" ? 22 : remote.data.secure ? 443 : 80);
  gameDefaultField(remote.data, "version", null);
  if (remote.type === "WebSocket") {
    gameDefaultField(remote.data, "secure", false);
    gameDefaultField(remote.data, "identity", "");
  } else {
    gameDefaultField(remote.data, "user", "");
    gameDefaultField(remote.data, "command", "");
    gameDefaultField(remote.data, "auth", { type: "Password" });
    if (remote.data.auth?.type === "PrivateKey") gameDefaultField(remote.data.auth.data, "path", "");
  }
}

/** P2-09 fixture default; cache behavior remains in Rust. */
export const defaultCacheEnabled = true;

/** P2-14 scripted reply defaults; no update, logging or installation logic. */
export const defaultUpdateCheck: Required<UpdateCheck> = {
  available: false, version: null, notes: null, published_at: null,
};
export const defaultUpdateDownloadStatus: Required<UpdateDownloadStatus> = {
  phase: "Idle", downloaded_bytes: 0, total_bytes: null, version: null, error: null,
};
export const defaultFullLogStatus: Required<FullLogStatus> = {
  enabled: false, remaining_seconds: 0,
};

/** P2-06 fixture defaults; Rust owns validation and persistence. */
export const defaultDisplaySettings: Required<import("../src/api/backend").DisplaySettingsPayload> = {
  estimate_location: "Off",
  estimate_style: "TransparentSmall",
  pure_estimate_style: "BigSquare",
  remember_estimate: false,
  moves_ownership: true,
  estimate_shortcut: false,
  sub_board_heat: "None",
  sub_board_raw: false,
  top_toolbar_layout: "Separate",
  top_toolbar_wrap: false,
  top_toolbar_hidden: [],
  bottom_toolbar_hidden: [],
  show_coordinates: true,
  move_numbers: "None",
  last_move_numbers: 1,
  move_numbers_from_one: false,
  branch_move_numbers_from_one: true,
  show_all_branch_move_numbers: false,
  move_rank_marks: "LastOne",
  last_move_rank_marks: 3,
  show_winrate_in_suggestion: true,
  show_visits_in_suggestion: true,
  show_score_in_suggestion: true,
  show_score_as_diff: false,
  show_suggestion_order: true,
  winrate_always_black: false,
  show_hover_variation: true,
  show_sub_board: true,
  show_list_pane: true,
  max_suggestions: 10,
  suggestion_max_red: false,
  next_moves: "Evaluation",
  next_move_min_visits: 30,
  show_winrate_graph: true,
  show_comment: true,
  show_variation_graph: true,
  show_information_panel: true,
  show_status_bar: true,
  show_top_toolbar: true,
  show_bottom_toolbar: true,
  winrate_graph_mode: "Both",
  fill_winrate_graph: true,
  show_blunder_bar: false,
  score_lead_includes_komi: true,
  confirm_new_board: true,
  confirm_overwrite: true,
  show_names_below_board: true,
};


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

/**
 * Every move of a 211-move game for one side, with losses from a fixed sequence: mostly small,
 * a few mistakes, Black weaker in the middle game and White in the endgame.
 */
function strengthSamples(black: boolean): Sample[] {
  const samples: Sample[] = [];
  let seed = black ? 7 : 13;
  for (let move = black ? 1 : 2; move <= 211; move += 2) {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    const roll = seed / 2147483648;
    const weak = black ? move > 60 && move <= 150 : move > 150;
    const loss = roll < (weak ? 0.12 : 0.05) ? 3 + roll * 40 : roll < 0.3 ? 0.6 + roll * 2 : roll * 0.5;
    const category = loss >= 6 ? "Blunder" : loss >= 3 ? "Mistake" : loss >= 1.2 ? "Inaccuracy" : loss >= 0.5 ? "Good" : loss >= 0.2 ? "Great" : "Excellent";
    samples.push({
      move_number: move,
      is_black: black,
      coordinate: "D4",
      winrate_loss: loss * 2.4,
      score_loss: loss,
      first_choice: loss < 0.2,
      ai_rank: loss < 0.2 ? 0 : 3,
      category,
      move_rank: 0,
      score_equivalent_loss: loss,
      complexity: 0.4,
      adjusted_weight: 1,
    });
  }
  return samples;
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
  black.samples = strengthSamples(true);
  white.samples = strengthSamples(false);
  return {
    revision,
    report: { black, white, overall: black, has_enough_data: true },
    phases: strengthPhases(),
    total_moves: 211,
    analyzed_moves: analyzed,
  };
}

/** Scripted 60/150 split for the phases tab; estimates and extrema are fixed data. */
export function strengthPhases(): StrengthPhasesReport {
  const blackOpening = strengthSide({
    rank: 6.4, zh: "业余6.4段", en: "6.4 dan", band: "5-6d", match: 0.82, first: 0.6,
    tiers: [18, 7, 3, 1, 1, 0],
    phases: [phase(30, 0.42, 0.6), phase(0, 0.42, 0.6), phase(0, 0.42, 0.6)],
  });
  const whiteOpening = strengthSide({
    rank: 4.2, zh: "业余4.2段", en: "4.2 dan", band: "3-4d", match: 0.68, first: 0.43,
    tiers: [13, 7, 5, 3, 1, 1],
    phases: [phase(30, 0.72, 0.43), phase(0, 0.72, 0.43), phase(0, 0.72, 0.43)],
  });
  const blackMiddle = strengthSide({
    rank: 4.1, zh: "业余4.1段", en: "4.1 dan", band: "3-4d", match: 0.64, first: 0.4,
    tiers: [18, 11, 7, 5, 3, 1],
    phases: [phase(0, 1.18, 0.4), phase(45, 1.18, 0.4), phase(0, 1.18, 0.4)],
  });
  const whiteMiddle = strengthSide({
    rank: 5.2, zh: "业余5.2段", en: "5.2 dan", band: "5-6d", match: 0.76, first: 0.51,
    tiers: [23, 12, 6, 2, 1, 1],
    phases: [phase(0, 0.64, 0.51), phase(45, 0.64, 0.51), phase(0, 0.64, 0.51)],
  });
  const blackEndgame = strengthSide({
    rank: 5.7, zh: "业余5.7段", en: "5.7 dan", band: "5-6d", match: 0.78, first: 0.55,
    tiers: [17, 8, 4, 1, 1, 0],
    // The existing model's fixed 160 boundary stays distinct from the tab's 150.
    phases: [phase(0, 0.58, 0.55), phase(5, 0.63, 0.6), phase(26, 0.57, 0.54)],
  });
  const whiteEndgame = strengthSide({
    rank: 3.7, zh: "业余3.7段", en: "3.7 dan", band: "3-4d", match: 0.62, first: 0.37,
    tiers: [11, 8, 5, 3, 2, 1],
    phases: [phase(0, 1.09, 0.37), phase(5, 1.26, 0.4), phase(25, 1.06, 0.36)],
  });
  return {
    bounds: [60, 150],
    phases: [
      { index: 0, first: 1, last: 60, played: true, black: blackOpening, white: whiteOpening, black_ranked: true, white_ranked: true },
      { index: 1, first: 61, last: 150, played: true, black: blackMiddle, white: whiteMiddle, black_ranked: true, white_ranked: true },
      { index: 2, first: 151, last: 211, played: true, black: blackEndgame, white: whiteEndgame, black_ranked: true, white_ranked: true },
    ],
    black: { best: 0, worst: 1, worth_reviewing: true },
    white: { best: 1, worst: 2, worth_reviewing: true },
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
    file_name: null,
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

/**
 * The middle game inside a recorded game: both players named, the last move ranked, and two
 * recorded continuations. The main line goes where the engine is not looking, so the board shows
 * what it costs; the branch follows the engine's second choice.
 */
export function recordedGame(revision: number) {
  const view = midGame(graphView(22, 20, { 19: [0.55, 1.4], 20: [0.562, 1.6], 21: [0.4, -2.5] }));
  view.game_info.black_name = "Black Player";
  view.game_info.white_name = "White Player";
  const play = (x: number, y: number): MoveView => ({ type: "Play", data: { color: "Black", point: { x, y } } });
  const nodes: VariationNodeView[] = view.tree.main_line.map((id, index, line) => ({
    id,
    parent: index === 0 ? null : (line[index - 1] ?? null),
    children: index + 1 < line.length ? [line[index + 1]!] : [21, 22],
    move_number: index,
    mv: null,
    move_error: null,
  }));
  nodes.push(
    { id: 21, parent: 20, children: [], move_number: 21, mv: play(13, 7), move_error: null },
    { id: 22, parent: 20, children: [], move_number: 21, mv: play(6, 9), move_error: null },
  );
  const tree: VariationTreeView = { revision: view.tree.structure_revision, root: 0, nodes };
  const problems: ProblemMovesView = {
    revision,
    settings: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
    query: { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" },
    entries: [],
    error_bar: view.graph.points.map((point) => ({
      node_id: point.node_id,
      move_number: point.move_number,
      mv: null,
      loss:
        point.node_id === 20
          ? { winrate_loss: 4.2, score_loss: 1.1, rank: "Inaccuracy", is_best: false, candidate_rank: 3, visits: 1200, before_visits: 1500 }
          : null,
    })),
    current_index: 20,
  };
  return { view, tree, problems, analysis: midGameAnalysis(revision, 20) };
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
    phases: {
      bounds: [60, 150],
      phases: [
        { index: 0, first: 1, last: 0, played: false, black: structuredClone(side), white: structuredClone(side), black_ranked: false, white_ranked: false },
        { index: 1, first: 61, last: 0, played: false, black: structuredClone(side), white: structuredClone(side), black_ranked: false, white_ranked: false },
        { index: 2, first: 151, last: 0, played: false, black: structuredClone(side), white: structuredClone(side), black_ranked: false, white_ranked: false },
      ],
      black: { best: null, worst: null, worth_reviewing: false },
      white: { best: null, worst: null, worth_reviewing: false },
    },
  };
}

/** Fixed DTO defaults for old scripts, never a simulation of queue scheduling. */
export const defaultBatchAnalysisStatus: BatchAnalysisStatus = {
  phase: "Idle", items: [], current_index: null, reason: null,
};
export const defaultFlashAnalysisSettings: FlashAnalysisSettings = { visits: 1, always_override: false };

/** P3-09 fixture defaults only; all play commands are explicitly scripted. */
export const defaultGameSetup: Required<GameSetup> = {
  black: { type: "Human" },
  white: { type: "Ai", data: { type: "KataGo", data: {
    seconds: 2, visits: null, pure_network: false,
    resign: { after_moves: 150, consecutive: 2, below_winrate: 0.05 }, random_opening: null,
  } } },
  start: { type: "NewBoard", data: {
    width: 19, height: 19, komi: 7.5, handicap: 0, rules: "chinese",
    black_name: null, black_rank: null, white_name: null, white_rank: null,
  } },
  human_clock: null, ponder: true, auto_save: true,
};
export const defaultGameView: Required<GameView> = {
  phase: "Playing", black: defaultGameSetup.black, white: defaultGameSetup.white,
  to_play: "Black", ai_thinking: false, black_clock: null, white_clock: null,
  move_number: 0, start_node: 0, latest_node: 0, result: null, end_reason: null, error: null,
};

export function fillGameSetupDefaults(setup: GameSetup): void {
  for (const key of Object.keys(defaultGameSetup) as (keyof GameSetup)[]) gameDefaultField(setup, key, structuredClone(defaultGameSetup[key]));
  for (const player of [setup.black, setup.white]) {
    if (player?.type === "Ai" && player.data.type === "KataGo") {
      const defaults = defaultGameSetup.white;
      if (defaults.type === "Ai" && defaults.data.type === "KataGo") {
        for (const key of Object.keys(defaults.data.data) as (keyof typeof defaults.data.data)[]) gameDefaultField(player.data.data, key, structuredClone(defaults.data.data[key]));
      }
    }
    if (player?.type === "Ai" && player.data.type === "HumanSl") {
      gameDefaultField(player.data.data, "rank", { type: "Dan", data: 3 });
      gameDefaultField(player.data.data, "pace", "Natural");
    }
  }
  if (setup.human_clock) {
    gameDefaultField(setup.human_clock, "main_seconds", 1200);
    gameDefaultField(setup.human_clock, "byoyomi_seconds", 30);
    gameDefaultField(setup.human_clock, "periods", 3);
  }
}

export function fillGameDefaults(view: GameView): void {
  for (const key of Object.keys(defaultGameView) as (keyof GameView)[]) gameDefaultField(view, key, structuredClone(defaultGameView[key]));
}

function gameDefaultField<T extends object, K extends keyof T>(target: T, key: K, value: T[K]): void {
  if (target[key] === undefined) Object.defineProperty(target, key, { value, writable: true, configurable: true });
}

/** Fixed mock DTO defaults; Tencent parsing and rank conversion stay in Rust. */
export function fillTencentPageDefaults(page: TencentPage): void {
  gameDefaultField(page, "games", []);
  gameDefaultField(page, "next_cursor", null);
  for (const game of page.games ?? []) {
    gameDefaultField(game, "id", "");
    gameDefaultField(game, "start_time", null);
    gameDefaultField(game, "move_count", 0);
    gameDefaultField(game, "black", {});
    gameDefaultField(game, "white", {});
    gameDefaultField(game, "result", { winner: null, kind: "Other", score: null, unit: null });
    for (const player of [game.black, game.white]) {
      if (!player) continue;
      gameDefaultField(player, "uid", "");
      gameDefaultField(player, "nickname", "");
      gameDefaultField(player, "rank", { type: "Kyu", data: 18 });
    }
  }
}


export const defaultMatchView: Required<MatchView> = {
  phase: "Running", games: 1, completed: 0, current_game: 0, a_is_black: true,
  a: { total: 0, black: 0, white: 0 }, b: { total: 0, black: 0, white: 0 },
  draws: 0, unscored: {}, openings: [], current_opening: null, elo: null, error: null, save_directory: null,
};
export function fillMatchDefaults(view: MatchView): void {
  for (const key of Object.keys(defaultMatchView) as (keyof MatchView)[]) gameDefaultField(view, key, structuredClone(defaultMatchView[key]));
  for (const side of [view.a, view.b]) {
    if (side) for (const key of ["total", "black", "white"] as const) gameDefaultField(side, key, 0);
  }
  for (const opening of view.openings ?? []) {
    gameDefaultField(opening, "opening", ""); gameDefaultField(opening, "completed", 0);
    gameDefaultField(opening, "draws", 0); gameDefaultField(opening, "unscored", {});
    gameDefaultField(opening, "a", { total: 0, black: 0, white: 0 });
    gameDefaultField(opening, "b", { total: 0, black: 0, white: 0 });
    for (const side of [opening.a, opening.b]) {
      if (side) for (const key of ["total", "black", "white"] as const) gameDefaultField(side, key, 0);
    }
  }
  if (view.elo) {
    gameDefaultField(view.elo, "elo", 0);
    gameDefaultField(view.elo, "lower", null); gameDefaultField(view.elo, "upper", null);
  }
}
export function fillGtpLineDefaults(line: GtpConsoleLine): void {
  gameDefaultField(line, "engine_id", ""); gameDefaultField(line, "sequence", 0);
  gameDefaultField(line, "direction", "Received"); gameDefaultField(line, "text", "");
}

/** P3-12 fixed snapshots only. Ratings, move selection and counters are scripted. */
export const defaultCoachSetup: Required<CoachSetup> = {
  rank: { type: "Dan", data: 3 }, mode: "LiveFeedback", color: "Random", pace: "Natural",
  handicap: 0, from_current_position: false, width: 19, height: 19, auto_save: true,
};
export const defaultCoachView: Required<CoachView> = {
  active: false,
  setup: defaultCoachSetup, human_color: "Black", awaiting_decision: false,
  hints: 0, take_backs: 0, last_rating: null, summary: null,
};
const defaultCoachRating: Required<CoachMoveRated> = {
  node_id: 0, move_number: 0, color: "Black", verdict: "Good", winrate_before: 0.5,
  winrate_after: 0.5, winrate_change: 0, points_lost: 0, human_share: null,
  best: { mv: { type: "Pass", data: { color: "Black" } }, human_share: null },
};
const defaultCoachSummary: Required<CoachSummary> = {
  result: null, end_reason: null, total_moves: 0, blunders: 0, hints: 0, take_backs: 0, worst_moves: [],
};
export function fillCoachSetupDefaults(setup: CoachSetup): void {
  for (const key of Object.keys(defaultCoachSetup) as (keyof CoachSetup)[]) gameDefaultField(setup, key, structuredClone(defaultCoachSetup[key]));
}
export function fillCoachRatingDefaults(rating: CoachMoveRated): void {
  for (const key of Object.keys(defaultCoachRating) as (keyof CoachMoveRated)[]) gameDefaultField(rating, key, structuredClone(defaultCoachRating[key]));
  if (rating.best) {
    gameDefaultField(rating.best, "mv", { type: "Pass", data: { color: "Black" } });
    gameDefaultField(rating.best, "human_share", null);
  }
}
export function fillCoachSummaryDefaults(summary: CoachSummary): void {
  for (const key of Object.keys(defaultCoachSummary) as (keyof CoachSummary)[]) gameDefaultField(summary, key, structuredClone(defaultCoachSummary[key]));
  for (const move of summary.worst_moves ?? []) {
    gameDefaultField(move, "node_id", 0); gameDefaultField(move, "move_number", 0);
    gameDefaultField(move, "verdict", "Good"); gameDefaultField(move, "points_lost", 0);
  }
}
export function fillCoachDefaults(view: CoachView): void {
  for (const key of Object.keys(defaultCoachView) as (keyof CoachView)[]) gameDefaultField(view, key, structuredClone(defaultCoachView[key]));
  fillCoachSetupDefaults(view.setup!);
  if (view.last_rating) fillCoachRatingDefaults(view.last_rating);
  if (view.summary) fillCoachSummaryDefaults(view.summary);
}

/** Commentary snapshots are fixed fixtures; all service/key actions are scripted. */
export const defaultCommentarySettings: Required<CommentarySettings> = {
  base_url: "", model: "", remember_key: false, rank: "5k", style: "Balanced",
  terminology: "Medium", detail: "Standard", variations: "Moderate", write_to_comment: true,
};
export const defaultCommentaryUpdated: Required<CommentaryUpdated> = {
  request_id: 0, phase: "Stopped", text: "", delta: "", replacement: false,
  positions: [], evidence: "", analyzed: 0, selected: 0, omitted: 0, issues: [], error: null,
};
export function fillCommentarySettingsDefaults(settings: CommentarySettings): void {
  for (const key of Object.keys(defaultCommentarySettings) as (keyof CommentarySettings)[]) gameDefaultField(settings, key, structuredClone(defaultCommentarySettings[key]));
}
export function fillCommentaryDefaults(view: CommentaryUpdated): void {
  for (const key of Object.keys(defaultCommentaryUpdated) as (keyof CommentaryUpdated)[]) gameDefaultField(view, key, structuredClone(defaultCommentaryUpdated[key]));
  for (const position of view.positions ?? []) {
    gameDefaultField(position, "node_id", 0); gameDefaultField(position, "move_number", 0);
  }
}

export const defaultCommentaryKeyStatus: Required<CommentaryKeyStatus> = { has_key: false, stored: false, available: false };
export function fillCommentaryKeyDefaults(status: CommentaryKeyStatus): void {
  for (const key of Object.keys(defaultCommentaryKeyStatus) as (keyof CommentaryKeyStatus)[]) gameDefaultField(status, key, defaultCommentaryKeyStatus[key]);
}
