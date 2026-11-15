import { fillZhiziDefaults } from "./fixtures";
import type { CommentaryUpdated, CommentarySettings, CommentaryKeyStatus, CoachView, CoachHint, HumanSlModelStatus, GtpEngineInfo, GtpConsoleLine, MatchView, GameView, BatchAnalysisStatus, FlashAnalysisSettings, AccelerationPlan, AssetDownloadStatus, EngineDiagnostics, OfficialWeight, AnalysisView, AnalysisLimitSettings, AnalysisState, Backend, BenchmarkStatus, BoardSyncStatus, BoardView, CommandError, DocumentView, EngineSettings, ProblemMovesView, Settings, StrengthEstimateView, UpdateCheck, UpdateDownloadStatus, FullLogStatus, VariationNodeView, VariationTreeView, WholeGameProgress, WireAppEvent } from "../src/api/backend";
import { fillRemoteProfileDefaults, fillCommentaryKeyDefaults, defaultCommentarySettings, fillCommentarySettingsDefaults, fillCommentaryDefaults, defaultCoachSetup, fillCoachSetupDefaults, fillCoachDefaults, fillCoachRatingDefaults, fillCoachSummaryDefaults, fillMatchDefaults, fillGtpLineDefaults, fillGameSetupDefaults, fillGameDefaults, defaultGameSetup, defaultGameView, defaultBatchAnalysisStatus, defaultFlashAnalysisSettings, defaultCacheEnabled, defaultDisplaySettings, defaultFullLogStatus, defaultUpdateCheck, defaultUpdateDownloadStatus, emptyStrengthEstimate } from "./fixtures";
import type { TencentPage } from "../src/api/backend";
import { fillTencentPageDefaults } from "./fixtures";

export type {
  ZhiziCommands, ZhiziStatus, ZhiziVerificationPurpose, ZhiziCodeDelivery, ZhiziOverview, ZhiziUsagePage, ZhiziCreditPage, ZhiziEngineOptions, ZhiziPlan, ZhiziWeight, ZhiziError, ZhiziErrorKind,
  TencentCommands, TencentQuery, TencentPage, TencentGame, TencentPlayer, TencentRank, TencentFailure, OpenUrlFailure,
  RemoteCommands, RemoteEngineInput, RemoteSshAuth, SshAuthConfig, RemoteEngineConfig, RemoteEngineError, RemoteLinkError,
  CommentaryCommands, CommentaryUpdatedListener, CommentaryRequest, CommentarySettings, CommentaryUpdated, CommentaryKeyStatus, CommentaryMode, CommentaryPhase, CommentaryPosition, CommentaryError,
  CommentaryStyle, CommentaryTerminology, CommentaryDetail, CommentaryVariations,
  CoachCommands, CoachUpdatedListener, CoachMoveRatedListener, CoachSummaryListener, CoachSetup, CoachView, CoachDecision, CoachHint, CoachMoveRated, CoachSummary, CoachBestMove, CoachWorstMove, CoachVerdict, CoachRank, CoachMode, CoachColor, CoachPace, CoachError, HumanSlModelStatus, HumanSlPlay,
  GtpCommands, GtpConsoleListener, GtpEngineEntry, GtpEngineInfo, GtpConsoleLine, GtpTime, MatchCommands, MatchUpdatedListener, MatchSetup, MatchView, MatchPlayer, MatchSide,
  GameCommands, GameUpdatedListener, GameSetup, GameView, GameStart, GamePhase, GameEndReason, GameError, AiPlayer, Player, KataGoPlay, HumanClock, HumanClockView, RandomOpening, ResignRule, PvPick,
  Backend, AppEvent, EngineProfile, EngineSetupProfile, EngineDiscovery, MoveView, Revision, Settings, EventListener,
  AnalysisSource, AnalysisSummary, AnalysisView, BoardView, CandidateView, ColorView, DocumentView,
  DocumentWarningView, EngineStatus, PointView, TreeView, WarningKindView, SubscriptionId,
  GraphPointView, GraphView,
  FoxAccount, FoxGame, FoxPage, FoxQuery, FoxSearchResult,
  FoxPlayer, FoxResult, FoxResultKind, FoxColor, FoxScoreUnit, FoxFailure, CommandError,
  AutoSetupError, AutoSetupAttempt, EngineDisplayName, OpenFailed, OpenFailedListener,
  GameInfoView, PreviewStoneView, VariationNodeView, VariationPreviewView, VariationTreeView,
  WireAppEvent, WholeGameOptions, WholeGamePreset, WholeGameProgress, WholeGameStatus, WholeGameSource,
  WholeGameProgressListener, WholeGameScope, FlashAnalysisSettings,
  BatchAnalysisCommands, BatchAnalysisProgressListener, BatchAnalysisStatus, BatchOptions, BatchOutput, BatchPhase, BatchItem, BatchItemState, BatchMove, BatchError,
  BoardSyncCommands, BoardSyncOptions, BoardSyncStatus, BoardSyncPhase, BoardSyncError,
  BoardSyncLanguage, BoardSyncStatusListener,
  BenchmarkCommands, BenchmarkStatus, BenchmarkPhase, BenchmarkProgress,
  BenchmarkMeasurement, BenchmarkResult, BenchmarkProgressListener,
  AnalysisLimitSettings, AnalysisState, AnalysisPauseReason, AnalysisLimitCommands, AnalysisStateListener,
  AnalysisExtras, AnalysisExtrasCommands,
  MoveRank, MoveRankEvaluationMode, MoveRankSettings, MoveLoss, ProblemMovesQuery,
  ProblemMovesView, ProblemMoveSort, ProblemMoveView, MistakeBarPoint,
  StrengthEstimateView, StrengthReport, SideReport, PhaseReport, Confidence, Sample, MoveCategory,
  TsumegoOptions, TsumegoStatus, TsumegoToPlay, TsumegoAttacker, TsumegoKoThreat,
  StrengthPhasesReport, PhaseEstimate, PhaseExtremes,
  EditCommands, GameInfoEdit, NewGameOptions,
  SetupMarkupCommands, MarkupKindView, MarkupView,
  MoveEditingCommands, StoneOriginView,
  DisplaySettings, DisplaySettingsPayload, EstimateLocation, EstimateStyle, SubBoardHeat, TopToolbarLayout, DisplaySettingsCommands, AnalysisCacheCommands,
  EngineCommands, EngineEntry, EngineSettings, EngineRules, EngineCustomRules, JavaImportPreview, JavaImportedEngine,
  WeightCommands, OfficialWeight, AssetDownloadStatus, AssetDownloadProgressListener, AccelerationPlan, DiagnosticRepair, EngineDiagnostics,
  AccelerationReason, DownloadPhase, DiagnosticCheck, DiagnosticKind, DiagnosticStatus,
  PreferenceCommands, UiLanguage,
  EverydayCommands, BoardTransform,
  UpdateCommands, UpdateCheck, UpdateDownloadStatus, UpdatePhase, UpdateDownloadProgressListener,
  SupportCommands, FullLogStatus,
  MoveNumberMode, MoveRankMarkMode, NextMoveHint, WinrateGraphMode,
} from "../src/api/backend";

type CommandName = Exclude<keyof Backend, "subscribeCommentaryUpdated" | "subscribeCoachUpdated" | "subscribeCoachMoveRated" | "subscribeCoachSummary" | "subscribeMatchUpdated" | "subscribeGtpConsole" | "subscribeGameUpdated" | "subscribe" | "subscribeBatchAnalysisProgress" | "subscribeWholeGameProgress" | "subscribeBoardSyncStatus" | "subscribeBenchmarkProgress" | "subscribeAnalysisState" | "subscribeOpenFailed" | "subscribeAssetDownloadProgress" | "subscribeUpdateDownloadProgress" | "unsubscribe" | "setWindowTitle">;
// Older UI fixtures can omit new settings fields without weakening the
// production Settings DTO. These are fixture defaults, not ranking logic.
type MockResult<K extends CommandName> = K extends "settings"
  ? Omit<Settings, "fox_history" | "move_rank" | "tsumego" | "auto_quick_analyze_on_load"> & Partial<Pick<Settings, "fox_history" | "move_rank" | "tsumego" | "auto_quick_analyze_on_load">>
  : Awaited<ReturnType<NonNullable<Backend[K]>>>;
const defaultMoveRank: Settings["move_rank"] = {
  mode: "Auto",
  winrate_thresholds: [1, 3, 6, 12, 24],
  score_thresholds: [0.5, 1.5, 3, 6, 12],
};
const defaultTsumego: Settings["tsumego"] = {
  to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1,
};
const defaultAnalysisLimits: AnalysisLimitSettings = {
  limit_time: true, max_analyze_time_seconds: 600, limit_playout: false, limit_playouts: 100000,
};
const defaultEngineSettings: EngineSettings = {
  rules: { type: "FollowSgf" }, read_komi: true, default_komi: 7.5,
  auto_check_alive: true, report_interval_ms: 100,
};
const defaultAnalysisState: AnalysisState = { pondering: true, pause_reason: null };
export type MockCall = {
  [K in CommandName]: { command: K; args: Parameters<NonNullable<Backend[K]>> }
}[CommandName];

export type MockCommand = {
  [K in CommandName]: {
    command: K;
    result: MockResult<K>;
    events?: WireAppEvent[];
    error?: string | CommandError;
  }
}[CommandName];

export interface MockScript {
  initialEvents?: WireAppEvent[];
  commands?: MockCommand[];
}

const listeners = new Map<number, (event: WireAppEvent) => void>();
let nextListenerId = 0;
let pendingEvents: WireAppEvent[] = [];
let commands: MockCommand[] = [];
const calls: MockCall[] = [];
let dispatching = false;
/** The newest document delivered to the app, for answering unscripted read-only queries. */
let lastDocument: { revision: number; view: DocumentView } | null = null;
let lastGameView: GameView | null = null;
let lastProgress: WholeGameProgress | null = null;
let lastBatchStatus = structuredClone(defaultBatchAnalysisStatus);
let lastFlashSettings = structuredClone(defaultFlashAnalysisSettings);
let lastAutoQuickAnalyzeOnLoad = true;
let lastCacheEnabled = defaultCacheEnabled;
let lastAnalysisLimits = structuredClone(defaultAnalysisLimits);
let lastAnalysisState = structuredClone(defaultAnalysisState);
let lastBoardSyncStatus: BoardSyncStatus = { phase: "Stopped", always_sync: true, always_goto_last: false, remote_move_number: null, error: null };
const idleBenchmark: BenchmarkStatus = { phase: "Idle", progress: null, measurements: [], last_result: null, error: null };
let lastBenchmarkStatus: BenchmarkStatus = structuredClone(idleBenchmark);
const idleDownload: AssetDownloadStatus = { phase: "Idle", asset_id: null, downloaded_bytes: 0, total_bytes: 0, path: null, error: null };
const defaultAcceleration: AccelerationPlan = { reason: "UnsupportedPlatform", recommended_assets: [], optional_assets: [], runtime_profile: null, tensor_rt_supported: false, tensor_rt_recommended: false, archives_only: false };
const emptyDiagnostics: EngineDiagnostics = { profile: null, checks: [] };
let lastDownloadStatus: AssetDownloadStatus = structuredClone(idleDownload);
/** A test-provided problem-move view, answered for every unscripted read. */
let problemMovesView: ProblemMovesView | null = null;

export function setProblemMoves(view: ProblemMovesView | null): void {
  problemMovesView = structuredClone(view);
}

/** A test-provided strength estimate, answered for every unscripted read. */
let strengthView: StrengthEstimateView | null = null;

export function setStrengthEstimate(view: StrengthEstimateView | null): void {
  strengthView = structuredClone(view);
}

/** Install deterministic replies and events. No Go or analysis logic is simulated. */
export function setScript(script: MockScript): void {
  lastGameView = null;
  lastBatchStatus = structuredClone(defaultBatchAnalysisStatus);
  lastFlashSettings = structuredClone(defaultFlashAnalysisSettings);
  lastAutoQuickAnalyzeOnLoad = true;
  lastCacheEnabled = defaultCacheEnabled;
  lastAnalysisLimits = structuredClone(defaultAnalysisLimits);
  lastAnalysisState = structuredClone(defaultAnalysisState);
  lastBoardSyncStatus = { phase: "Stopped", always_sync: true, always_goto_last: false, remote_move_number: null, error: null };
  lastBenchmarkStatus = structuredClone(idleBenchmark);
  lastDownloadStatus = structuredClone(idleDownload);
  pendingEvents = structuredClone(script.initialEvents ?? []);
  commands = structuredClone(script.commands ?? []);
  calls.length = 0;
  if (listeners.size !== 0) pushEvents([]);
}

export function recordedCalls(): MockCall[] {
  return structuredClone(calls);
}

/** Push document, analysis and status events in exactly the scripted order. */
export function pushEvents(events: readonly WireAppEvent[]): void {
  pendingEvents.push(...structuredClone(events));
  if (listeners.size === 0 || dispatching) return;
  dispatching = true;
  try {
    while (listeners.size !== 0 && pendingEvents.length !== 0) {
      const event = pendingEvents.shift()!;
      // A fixed fixture value for older scripts; never infer speed from visits.
      // Explicit null and zero remain available for warm-up and stopped cases.
      if (event.type === "AnalysisUpdated" && event.data.visits_per_second === undefined) {
        event.data.visits_per_second = event.data.source === "Live" ? 1200 : null;
      }
      if (event.type === "DocumentChanged") {
        event.data.view.file_name ??= null;
        lastDocument = structuredClone(event.data);
        fillDocumentDefaults(lastDocument.view);
      }
      if (event.type === "BatchAnalysisProgress") lastBatchStatus = { ...defaultBatchAnalysisStatus, ...event.data };
      if (event.type === "GameUpdated") lastGameView = { ...structuredClone(defaultGameView), ...event.data };
      if (event.type === "WholeGameProgress") lastProgress = structuredClone(event.data);
      if (event.type === "BoardSyncStatusChanged") lastBoardSyncStatus = structuredClone(event.data);
      if (event.type === "BenchmarkProgress") lastBenchmarkStatus = structuredClone(event.data);
      if (event.type === "AssetDownloadProgress") {
        fillDownloadDefaults(event.data);
        lastDownloadStatus = { ...idleDownload, ...event.data };
      }
      if (event.type === "AnalysisStateChanged") lastAnalysisState = structuredClone(event.data);
      for (const listener of [...listeners.values()]) {
        try {
          const cloned = structuredClone(event);
          if (cloned.type === "MatchUpdated") fillMatchDefaults(cloned.data);
          if (cloned.type === "GtpConsoleLine") fillGtpLineDefaults(cloned.data);
          if (cloned.type === "CommentaryUpdated") fillCommentaryDefaults(cloned.data);
          if (cloned.type === "CoachUpdated") fillCoachDefaults(cloned.data);
          if (cloned.type === "CoachMoveRated") fillCoachRatingDefaults(cloned.data);
          if (cloned.type === "CoachSummary") fillCoachSummaryDefaults(cloned.data);
          if (cloned.type === "GameUpdated") fillGameDefaults(cloned.data);
          if (cloned.type === "BatchAnalysisProgress") fillBatchDefaults(cloned.data);
          if (cloned.type === "DocumentChanged") fillDocumentDefaults(cloned.data.view);
          if (cloned.type === "AnalysisUpdated") fillAnalysisDefaults(cloned.data);
          if (cloned.type === "AnalysisStateChanged") defaultField(cloned.data, "pure_network", false);
          if (cloned.type === "AssetDownloadProgress") fillDownloadDefaults(cloned.data);
          if (cloned.type === "UpdateDownloadProgress") fillUpdateDownloadDefaults(cloned.data);
          listener(cloned);
        } catch (error) {
          console.error("Backend event listener failed", error);
        }
      }
    }
  } finally {
    dispatching = false;
  }
}

async function command<K extends CommandName>(
  name: K,
  args: Parameters<NonNullable<Backend[K]>>,
): Promise<Awaited<ReturnType<NonNullable<Backend[K]>>>> {
  calls.push(structuredClone({ command: name, args }) as MockCall);
  const next = commands[0];
  if (!next || next.command !== name) {
    throw new Error(`Expected mock command ${next?.command ?? "(none)"}, received ${name}`);
  }
  commands.shift();
  if (next.error !== undefined) {
    throw typeof next.error === "string" ? new Error(next.error) : structuredClone(next.error);
  }
  pushEvents(next.events ?? []);
  const result = next.command === "settings"
    ? {
      ...next.result,
      fox_history: next.result.fox_history ?? [],
      move_rank: next.result.move_rank ?? defaultMoveRank,
      tsumego: next.result.tsumego ?? defaultTsumego,
      benchmarks: next.result.benchmarks ?? {},
      auto_quick_analyze_on_load: next.result.auto_quick_analyze_on_load ?? true,
      cache_enabled: next.result.cache_enabled ?? defaultCacheEnabled,
      analysis_limits: next.result.analysis_limits ?? defaultAnalysisLimits,
      display: { show_list_pane: true, ...(next.result.display ?? defaultDisplaySettings) },
    }
    : next.result;
  if (next.command === "settings") {
    lastAutoQuickAnalyzeOnLoad = next.result.auto_quick_analyze_on_load ?? true;
    lastCacheEnabled = next.result.cache_enabled ?? defaultCacheEnabled;
    lastAnalysisLimits = structuredClone(next.result.analysis_limits ?? defaultAnalysisLimits);
  } else if (next.command === "autoQuickAnalyzeOnLoad" || next.command === "setAutoQuickAnalyzeOnLoad") {
    lastAutoQuickAnalyzeOnLoad = next.result;
  } else if (next.command === "cacheEnabled" || next.command === "setCacheEnabled") {
    lastCacheEnabled = next.result;
  } else if (next.command === "analysisLimits" || next.command === "setAnalysisLimits") {
    lastAnalysisLimits = structuredClone(next.result);
  } else if (next.command === "analysisState" || next.command === "togglePondering") {
    lastAnalysisState = structuredClone(next.result);
  }
  const cloned = structuredClone(result);
  if (next.command === "tencentSearch" || next.command === "tencentNextPage") fillTencentPageDefaults(cloned as TencentPage);
  if (next.command === "analysisState" || next.command === "togglePondering") defaultField(cloned as AnalysisState, "pure_network", false);
  if (next.command === "setDisplaySettings") fillDisplayDefaults(cloned as NonNullable<Settings["display"]>);
  if (next.command === "wholeGameResult" && cloned) fillAnalysisDefaults(cloned as AnalysisView);
  if (next.command === "problemMoves" && cloned) fillLossDefaults(cloned as ProblemMovesView);
  if (next.command === "variationTree") fillTreeDefaults(cloned as VariationTreeView);
  if (["startAssetDownload", "pauseAssetDownload", "cancelAssetDownload", "assetDownloadStatus"].includes(next.command)) {
    fillDownloadDefaults(cloned as AssetDownloadStatus);
  }
  if ((next.command === "checkUpdate" || next.command === "automaticUpdateCheck") && cloned) {
    const check = cloned as UpdateCheck;
    for (const key of Object.keys(defaultUpdateCheck) as (keyof UpdateCheck)[]) defaultField(check, key, defaultUpdateCheck[key]);
  }
  if (["downloadUpdate", "cancelUpdateDownload", "updateDownloadStatus", "installUpdate"].includes(next.command)) {
    fillUpdateDownloadDefaults(cloned as UpdateDownloadStatus);
  }
  if (["fullLogStatus", "startFullLog", "stopFullLog"].includes(next.command)) {
    const status = cloned as FullLogStatus;
    for (const key of Object.keys(defaultFullLogStatus) as (keyof FullLogStatus)[]) defaultField(status, key, defaultFullLogStatus[key]);
  }
  if (next.command === "officialWeights") {
    for (const weight of cloned as OfficialWeight[]) {
      defaultField(weight, "id", "");
      defaultField(weight, "name", "");
      defaultField(weight, "size_bytes", 0);
      defaultField(weight, "published_at", null);
      defaultField(weight, "downloaded", false);
      defaultField(weight, "path", null);
    }
  }
  if (next.command === "accelerationPlan") {
    const plan = cloned as AccelerationPlan;
    for (const key of Object.keys(defaultAcceleration) as (keyof AccelerationPlan)[]) defaultField(plan, key, defaultAcceleration[key]);
  }
  if (next.command === "diagnoseEngine" || next.command === "repairEngine") {
    const report = cloned as EngineDiagnostics;
    defaultField(report, "profile", null);
    defaultField(report, "checks", []);
    for (const check of report.checks ?? []) {
      defaultField(check, "status", "Skipped");
      defaultField(check, "path", null);
      defaultField(check, "detail", null);
      defaultField(check, "repairs", []);
    }
  }
  if (next.command === "settings") {
    const settings = cloned as Settings;
    fillDisplayDefaults(settings.display!);
    defaultField(settings, "flash_analysis", structuredClone(defaultFlashAnalysisSettings));
    fillFlashDefaults(settings.flash_analysis!);
    lastFlashSettings = { ...defaultFlashAnalysisSettings, ...settings.flash_analysis };
    defaultField(settings, "commentary", structuredClone(defaultCommentarySettings));
    fillCommentarySettingsDefaults(settings.commentary!);
    defaultField(settings, "coach_setup", structuredClone(defaultCoachSetup));
    fillCoachSetupDefaults(settings.coach_setup!);
    if (settings.engine) defaultField(settings.engine, "human_model", undefined);
    for (const entry of settings.engines ?? []) if (entry.profile) defaultField(entry.profile, "human_model", undefined);
    defaultField(settings, "game_setup", structuredClone(defaultGameSetup));
    fillGameSetupDefaults(settings.game_setup!);
    defaultField(settings, "games_directory", null);
    defaultField(settings, "gtp_engines", []);
    defaultField(settings, "tencent_history", []);
    defaultField(settings, "recent_files", []);
    // As Rust's normalize_engines: an old single engine is a one-item list, and the main one.
    const legacy = settings.engine ? [{ id: "legacy-engine", name: "KataGo", profile: settings.engine }] : [];
    defaultField(settings, "engines", legacy);
    if (settings.engine) fillRemoteProfileDefaults(settings.engine);
    for (const entry of settings.engines ?? []) if (entry.profile) fillRemoteProfileDefaults(entry.profile);
    defaultField(settings, "main_engine_id", settings.engine ? "legacy-engine" : null);
    defaultField(settings, "engine_settings", structuredClone(defaultEngineSettings));
    defaultField(settings, "language", "System");
    defaultField(settings, "always_on_top", false);
    defaultField(settings, "ui_scale", 1);
    defaultField(settings, "play_sound", true);
    defaultField(settings, "auto_check_updates", true);
    defaultField(settings, "last_update_check_unix_ms", null);
    const engine = settings.engine_settings!;
    defaultField(engine, "rules", { type: "FollowSgf" });
    defaultField(engine, "read_komi", true);
    defaultField(engine, "default_komi", 7.5);
    defaultField(engine, "auto_check_alive", true);
    defaultField(engine, "report_interval_ms", 100);
  }
  fillZhiziDefaults(next.command, cloned);
  if (["addRemoteEngine", "updateRemoteEngine", "switchToLocalEngine"].includes(next.command)) {
    const settings = cloned as Settings;
    if (settings.engine) fillRemoteProfileDefaults(settings.engine);
    for (const entry of settings.engines ?? []) if (entry.profile) fillRemoteProfileDefaults(entry.profile);
  }
  if (next.command === "flashAnalysis" || next.command === "setFlashAnalysis") {
    fillFlashDefaults(cloned as FlashAnalysisSettings);
    lastFlashSettings = { ...defaultFlashAnalysisSettings, ...cloned as FlashAnalysisSettings };
  }
  if (["startBatchAnalysis", "pauseBatchAnalysis", "resumeBatchAnalysis", "stopBatchAnalysis", "addBatchAnalysisFiles", "removeBatchAnalysisItem", "moveBatchAnalysisItem", "clearBatchAnalysis", "batchAnalysisStatus"].includes(next.command)) {
    fillBatchDefaults(cloned as BatchAnalysisStatus);
    lastBatchStatus = { ...defaultBatchAnalysisStatus, ...cloned as BatchAnalysisStatus };
  }
  if (["startMatch", "pauseMatch", "resumeMatch", "stopMatch", "setMatchGames", "matchPlayNow", "matchResign", "setMatchSideParams", "matchView"].includes(next.command) && cloned) fillMatchDefaults(cloned as MatchView);
  if (next.command === "gtpConsole" && cloned) (cloned as GtpConsoleLine[]).forEach(fillGtpLineDefaults);
  if (next.command === "testGtpEngine" && cloned) {
    const info = cloned as GtpEngineInfo;
    defaultField(info, "name", ""); defaultField(info, "version", ""); defaultField(info, "commands", []);
  }
  if (["startGame", "pauseGame", "resumeGame", "stopGame", "resignGame", "takeBack", "continueGame", "setAiPlayer", "gameView"].includes(next.command) && cloned) {
    fillGameDefaults(cloned as GameView);
    lastGameView = { ...structuredClone(defaultGameView), ...cloned as GameView };
  }
  if ((next.command === "startCoach" || next.command === "coachView") && cloned) {
    fillCoachDefaults(cloned as CoachView);
  }
  if (next.command === "humanslModelStatus" || next.command === "importHumanslModel") {
    const status = cloned as HumanSlModelStatus;
    if (status.type === "Downloading") {
      defaultField(status.data, "downloaded_bytes", 0); defaultField(status.data, "total_bytes", 99066230);
      defaultField(status.data, "paused", false);
    } else if (status.type === "Ready") defaultField(status.data, "path", "");
  }
  if (next.command === "coachHint" || next.command === "coachDecision") {
    const hint = cloned as CoachHint;
    if (hint.type === "Region") {
      defaultField(hint.data, "row", 0); defaultField(hint.data, "column", 0);
    } else if (hint.type === "Exact") {
      defaultField(hint.data, "mv", { type: "Pass", data: { color: "Black" } }); defaultField(hint.data, "human_share", null);
    }
  }
  if (["startCommentary", "stopCommentary", "commentaryStatus"].includes(next.command)) fillCommentaryDefaults(cloned as CommentaryUpdated);
  if (next.command === "setCommentarySettings") fillCommentarySettingsDefaults(cloned as CommentarySettings);
  if (["setCommentaryKey", "clearCommentaryKey", "commentaryKeyStatus"].includes(next.command)) fillCommentaryKeyDefaults(cloned as CommentaryKeyStatus);
  return cloned as Awaited<ReturnType<NonNullable<Backend[K]>>>;
}

/** Older scripts are also compared verbatim by existing API consumers.
 * Missing fields remain non-enumerable fixture defaults so those scripts keep
 * their original data shape, while UI property reads see the new Rust defaults.
 * Apply after cloning; explicit scripted values retain their normal shape. */
function defaultField<T extends object, K extends keyof T>(target: T, key: K, value: T[K]): void {
  if (target[key] === undefined) {
    Object.defineProperty(target, key, { value, writable: true, configurable: true });
  }
}

function fillDisplayDefaults(display: NonNullable<Settings["display"]>): void {
  for (const key of Object.getOwnPropertyNames(defaultDisplaySettings) as (keyof typeof defaultDisplaySettings)[]) {
    defaultField(display, key, structuredClone(defaultDisplaySettings[key]));
  }
}

function fillBatchDefaults(status: BatchAnalysisStatus): void {
  for (const key of Object.keys(defaultBatchAnalysisStatus) as (keyof BatchAnalysisStatus)[]) defaultField(status, key, defaultBatchAnalysisStatus[key]);
  for (const item of status.items ?? []) {
    defaultField(item, "id", 0); defaultField(item, "path", ""); defaultField(item, "state", { type: "Waiting" });
    if (item.state?.type === "Running") defaultField(item.state.data, "progress", null);
    if (item.state?.type === "Completed") defaultField(item.state.data, "output", "");
    if (item.state?.type === "Failed") defaultField(item.state.data, "reason", "");
  }
}

function fillFlashDefaults(settings: FlashAnalysisSettings): void {
  defaultField(settings, "visits", 1); defaultField(settings, "always_override", false);
}

function fillDownloadDefaults(status: AssetDownloadStatus): void {
  for (const key of Object.keys(idleDownload) as (keyof AssetDownloadStatus)[]) defaultField(status, key, idleDownload[key]);
}

function fillUpdateDownloadDefaults(status: UpdateDownloadStatus): void {
  for (const key of Object.keys(defaultUpdateDownloadStatus) as (keyof UpdateDownloadStatus)[]) defaultField(status, key, defaultUpdateDownloadStatus[key]);
}

function fillDocumentDefaults(view: DocumentView): void {
  defaultField(view, "file_path", null);
  defaultField(view, "modified", false);
  defaultField(view, "can_undo", false);
  defaultField(view, "can_redo", false);
  defaultField(view, "markup", []);
  defaultField(view, "trial", false);
  defaultField(view.game_info, "rules_preset", "chinese");
  defaultField(view.tree, "comment_revision", 0);
  defaultField(view.tree, "branch_start_move_number", null);
  for (const point of view.graph.points) {
    if (point.summary) defaultField(point.summary, "score_lead_available", true);
  }
  if (view.board) fillBoardDefaults(view.board);
}

function fillAnalysisDefaults(view: AnalysisView): void {
  defaultField(view, "policy", null);
  for (const candidate of view.candidates) defaultField(candidate, "ownership", null);
  defaultField(view.summary, "score_lead_available", true);
  for (const candidate of view.candidates) defaultField(candidate, "score_lead_available", true);
}

function fillLossDefaults(view: ProblemMovesView): void {
  for (const entry of view.entries) defaultField(entry.loss, "score_loss_available", true);
  for (const point of view.error_bar) {
    if (point.loss) defaultField(point.loss, "score_loss_available", true);
  }
}

function fillBoardDefaults(board: BoardView): void {
  defaultField(board, "stone_move_numbers", Array<null>(board.stones.length).fill(null));
  defaultField(board, "stone_node_ids", Array<null>(board.stones.length).fill(null));
}

function fillTreeDefaults(tree: VariationTreeView): void {
  defaultField(tree, "comment_revision", 0);
  for (const node of tree.nodes) defaultField(node, "has_comment", false);
}

export const startBatchAnalysis: Backend["startBatchAnalysis"] = (options) => command("startBatchAnalysis", [options]);
export const pauseBatchAnalysis: Backend["pauseBatchAnalysis"] = () => command("pauseBatchAnalysis", []);
export const resumeBatchAnalysis: Backend["resumeBatchAnalysis"] = () => command("resumeBatchAnalysis", []);
export const stopBatchAnalysis: Backend["stopBatchAnalysis"] = () => command("stopBatchAnalysis", []);
export const addBatchAnalysisFiles: Backend["addBatchAnalysisFiles"] = (files) => command("addBatchAnalysisFiles", [files]);
export const removeBatchAnalysisItem: Backend["removeBatchAnalysisItem"] = (id) => command("removeBatchAnalysisItem", [id]);
export const moveBatchAnalysisItem: Backend["moveBatchAnalysisItem"] = (id, direction) => command("moveBatchAnalysisItem", [id, direction]);
export const clearBatchAnalysis: Backend["clearBatchAnalysis"] = () => command("clearBatchAnalysis", []);
export const batchAnalysisStatus: Backend["batchAnalysisStatus"] = () => query("batchAnalysisStatus", [], () => lastBatchStatus);
export const flashAnalysis: Backend["flashAnalysis"] = () => query("flashAnalysis", [], () => lastFlashSettings);
export const setFlashAnalysis: Backend["setFlashAnalysis"] = (settings) => command("setFlashAnalysis", [settings]);

export const openBytes: Backend["openBytes"] = (bytes) => command("openBytes", [bytes]);
export const openPath: Backend["openPath"] = (path) => command("openPath", [path]);
export const save: Backend["save"] = (path) => command("save", [path]);
export const play: Backend["play"] = (mv) => command("play", [mv]);
export const step: Backend["step"] = (count) => command("step", [count]);
export const goTo: Backend["goTo"] = (nodeId) => command("goTo", [nodeId]);
export const undo: Backend["undo"] = () => command("undo", []);
export const redo: Backend["redo"] = () => command("redo", []);
export const setComment: Backend["setComment"] = (text) => command("setComment", [text]);
export const deleteNode: Backend["deleteNode"] = (nodeId) => command("deleteNode", [nodeId]);
export const deleteBranch: Backend["deleteBranch"] = (nodeId) => command("deleteBranch", [nodeId]);
export const promoteToMain: Backend["promoteToMain"] = (nodeId) => command("promoteToMain", [nodeId]);
export const newGame: Backend["newGame"] = (options) => command("newGame", [options]);
export const setGameInfo: Backend["setGameInfo"] = (info) => command("setGameInfo", [info]);
export const clearRecentFiles: Backend["clearRecentFiles"] = () => command("clearRecentFiles", []);
export const setSetupStone: Backend["setSetupStone"] = (point, color) => command("setSetupStone", [point, color]);
export const clearSetup: Backend["clearSetup"] = () => command("clearSetup", []);
export const setFirstPlayer: Backend["setFirstPlayer"] = (color) => command("setFirstPlayer", [color]);
export const convertToStartPosition: Backend["convertToStartPosition"] = () => command("convertToStartPosition", []);
export const setMarkup: Backend["setMarkup"] = (point, kind) => command("setMarkup", [point, kind]);
export const clearMarkup: Backend["clearMarkup"] = () => command("clearMarkup", []);
export const toggleToPlay: Backend["toggleToPlay"] = () => command("toggleToPlay", []);
export const playColored: Backend["playColored"] = (mv) => command("playColored", [mv]);
export const insertMove: Backend["insertMove"] = (mv) => command("insertMove", [mv]);
export const switchStone: Backend["switchStone"] = (point) => command("switchStone", [point]);
export const deleteStone: Backend["deleteStone"] = (point) => command("deleteStone", [point]);
export const moveStone: Backend["moveStone"] = (from, to) => command("moveStone", [from, to]);
export const addVariation: Backend["addVariation"] = (moves) => command("addVariation", [moves]);
export const stoneOrigin: Backend["stoneOrigin"] = (point) => query("stoneOrigin", [point], () => null);
export const startTrial: Backend["startTrial"] = () => command("startTrial", []);
export const endTrial: Backend["endTrial"] = () => command("endTrial", []);
export const generateTsumego: Backend["generateTsumego"] = (options) => command("generateTsumego", [options]);
export const exitTsumego: Backend["exitTsumego"] = () => command("exitTsumego", []);
export const tsumegoStatus: Backend["tsumegoStatus"] = () => command("tsumegoStatus", []);
/**
 * Read-only queries follow the script when it lists them next; otherwise they
 * are answered from the last document, unrecorded, so a UI that fetches its
 * tree after every structural change does not disturb scripted command lists.
 */
function query<K extends "gameView" | "setAnalysisExtras" | "batchAnalysisStatus" | "flashAnalysis" | "officialWeights" | "assetDownloadStatus" | "accelerationPlan" | "diagnoseEngine" | "variationTree" | "previewVariation" | "engineDisplayName" | "wholeGameProgress" | "boardSyncStatus" | "benchmarkStatus" | "problemMoves" | "strengthEstimate" | "autoQuickAnalyzeOnLoad" | "analysisLimits" | "analysisState" | "recentFiles" | "stoneOrigin" | "cacheEnabled" | "updateDownloadStatus" | "fullLogStatus" | "logDirectory" | "automaticUpdateCheck" | "alwaysOnTop" | "uiScale" | "playSound" | "clipboardLink">(
  name: K,
  args: Parameters<NonNullable<Backend[K]>>,
  fallback: () => Awaited<ReturnType<NonNullable<Backend[K]>>>,
): Promise<Awaited<ReturnType<NonNullable<Backend[K]>>>> {
  if (commands[0]?.command === name) return command(name, args);
  const cloned = structuredClone(fallback());
  if (name === "batchAnalysisStatus") fillBatchDefaults(cloned as BatchAnalysisStatus);
  if (name === "flashAnalysis") fillFlashDefaults(cloned as FlashAnalysisSettings);
  if (name === "problemMoves") fillLossDefaults(cloned as ProblemMovesView);
  return Promise.resolve(cloned);
}

/** The main line of the last document as a branchless tree; moves are unknown to the mock. */
function mainLineTree(): VariationTreeView {
  const tree = lastDocument?.view.tree;
  if (!tree) return { revision: 0, comment_revision: 0, root: 0, nodes: [] };
  const nodes: VariationNodeView[] = tree.main_line.map((id, index) => ({
    id,
    parent: index === 0 ? null : (tree.main_line[index - 1] ?? null),
    children: index + 1 < tree.main_line.length ? [tree.main_line[index + 1]!] : [],
    move_number: index,
    mv: null,
    move_error: null,
    has_comment: false,
  }));
  return { revision: tree.structure_revision, comment_revision: tree.comment_revision ?? 0, root: tree.root, nodes };
}

export const addEngine: Backend["addEngine"] = (name, profile) => command("addEngine", [name, profile]);
export const zhiziStatus: NonNullable<Backend["zhiziStatus"]> = () => command("zhiziStatus", []);
export const zhiziLoginPassword: NonNullable<Backend["zhiziLoginPassword"]> = (account, password, remember) => command("zhiziLoginPassword", [account, password, remember]);
export const zhiziLoginCode: NonNullable<Backend["zhiziLoginCode"]> = (account, code, remember) => command("zhiziLoginCode", [account, code, remember]);
export const zhiziSendCode: NonNullable<Backend["zhiziSendCode"]> = (account, purpose) => command("zhiziSendCode", [account, purpose]);
export const zhiziResetPassword: NonNullable<Backend["zhiziResetPassword"]> = (account, code, password, remember) => command("zhiziResetPassword", [account, code, password, remember]);
export const zhiziLogout: NonNullable<Backend["zhiziLogout"]> = () => command("zhiziLogout", []);
export const zhiziOverview: NonNullable<Backend["zhiziOverview"]> = (force) => command("zhiziOverview", [force]);
export const zhiziUsages: NonNullable<Backend["zhiziUsages"]> = (page) => command("zhiziUsages", [page]);
export const zhiziCredits: NonNullable<Backend["zhiziCredits"]> = (page) => command("zhiziCredits", [page]);
export const zhiziWebsite: NonNullable<Backend["zhiziWebsite"]> = () => command("zhiziWebsite", []);
export const openZhiziWebsite: NonNullable<Backend["openZhiziWebsite"]> = () => command("openZhiziWebsite", []);
export const addRemoteEngine: NonNullable<Backend["addRemoteEngine"]> = (name, entry) => command("addRemoteEngine", [name, entry]);
export const updateRemoteEngine: NonNullable<Backend["updateRemoteEngine"]> = (id, name, entry) => command("updateRemoteEngine", [id, name, entry]);
export const testRemoteEngine: NonNullable<Backend["testRemoteEngine"]> = (entry) => command("testRemoteEngine", [entry]);
export const switchToLocalEngine: NonNullable<Backend["switchToLocalEngine"]> = () => command("switchToLocalEngine", []);
export const normalizeRemoteLink: NonNullable<Backend["normalizeRemoteLink"]> = (text) => command("normalizeRemoteLink", [text]);
export const readRemoteLinkQr: NonNullable<Backend["readRemoteLinkQr"]> = (path) => command("readRemoteLinkQr", [path]);
export const updateEngine: Backend["updateEngine"] = (id, name, profile) => command("updateEngine", [id, name, profile]);
export const removeEngine: Backend["removeEngine"] = (id) => command("removeEngine", [id]);
export const reorderEngines: Backend["reorderEngines"] = (ids) => command("reorderEngines", [ids]);
export const selectMainEngine: Backend["selectMainEngine"] = (id) => command("selectMainEngine", [id]);
export const restartEngine: Backend["restartEngine"] = () => command("restartEngine", []);
export const closeEngine: Backend["closeEngine"] = () => command("closeEngine", []);
export const setEngineSettings: Backend["setEngineSettings"] = (settings) => command("setEngineSettings", [settings]);
export const previewJavaImport: Backend["previewJavaImport"] = (directory) => command("previewJavaImport", [directory]);
export const confirmJavaImport: Backend["confirmJavaImport"] = (previewId) => command("confirmJavaImport", [previewId]);
export const officialWeights: Backend["officialWeights"] = () => query("officialWeights", [], () => []);
export const startAssetDownload: Backend["startAssetDownload"] = (assetId) => command("startAssetDownload", [assetId]);
export const cancelAssetDownload: Backend["cancelAssetDownload"] = () => command("cancelAssetDownload", []);
export const assetDownloadStatus: Backend["assetDownloadStatus"] = () => query("assetDownloadStatus", [], () => lastDownloadStatus);
export const accelerationPlan: Backend["accelerationPlan"] = () => query("accelerationPlan", [], () => defaultAcceleration);
export const useOfficialWeight: Backend["useOfficialWeight"] = (assetId) => command("useOfficialWeight", [assetId]);
export const diagnoseEngine: Backend["diagnoseEngine"] = () => query("diagnoseEngine", [], () => emptyDiagnostics);
export const repairEngine: Backend["repairEngine"] = (repair) => command("repairEngine", [repair]);

export const recentFiles: Backend["recentFiles"] = () => query("recentFiles", [], () => []);
export const variationTree: Backend["variationTree"] = () => query("variationTree", [], mainLineTree);
export const previewVariation: Backend["previewVariation"] = (nodeId, moves) =>
  query("previewVariation", [nodeId, moves], () => {
    if (!lastDocument?.view.board) throw new Error("mock preview needs a document with a board");
    return { revision: lastDocument.revision, node_id: nodeId, board: lastDocument.view.board, applied_moves: [], stones: [], stop_reason: null };
  }).then((preview) => {
    fillBoardDefaults(preview.board);
    return preview;
  });
export const startWholeGameAnalysis: Backend["startWholeGameAnalysis"] = (options = { visits: 1000 }) =>
  command("startWholeGameAnalysis", [options]);
export const pauseWholeGameAnalysis: Backend["pauseWholeGameAnalysis"] = () =>
  command("pauseWholeGameAnalysis", []);
export const resumeWholeGameAnalysis: Backend["resumeWholeGameAnalysis"] = () =>
  command("resumeWholeGameAnalysis", []);
export const cancelWholeGameAnalysis: Backend["cancelWholeGameAnalysis"] = () =>
  command("cancelWholeGameAnalysis", []);
export const wholeGameProgress: Backend["wholeGameProgress"] = () => query("wholeGameProgress", [], () => lastProgress);
export const wholeGameResult: Backend["wholeGameResult"] = (nodeId) => command("wholeGameResult", [nodeId]);
export const startBoardSync: Backend["startBoardSync"] = (options = { always_sync: true, always_goto_last: false, language: "Cn" }) =>
  command("startBoardSync", [options]);
export const stopBoardSync: Backend["stopBoardSync"] = () => command("stopBoardSync", []);
export const boardSyncStatus: Backend["boardSyncStatus"] = () =>
  query("boardSyncStatus", [], () => lastBoardSyncStatus);
export const startBenchmark: Backend["startBenchmark"] = () => command("startBenchmark", []);
export const cancelBenchmark: Backend["cancelBenchmark"] = () => command("cancelBenchmark", []);
export const resetBenchmark: Backend["resetBenchmark"] = () => command("resetBenchmark", []);
export const benchmarkStatus: Backend["benchmarkStatus"] = () =>
  query("benchmarkStatus", [], () => lastBenchmarkStatus);
export const problemMoves: Backend["problemMoves"] =
  (filter = { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" }) =>
    query("problemMoves", [filter], () => problemMovesView ? { ...problemMovesView, query: filter } : ({
      revision: lastDocument?.revision ?? 0,
      settings: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
      query: filter,
      entries: [],
      // No losses without whole-game results: one empty bar per graph point.
      error_bar: (lastDocument?.view.graph.points ?? []).map((point) => ({
        node_id: point.node_id,
        move_number: point.move_number,
        mv: null,
        loss: null,
      })),
      current_index: lastDocument?.view.graph.current_index ?? 0,
    }));
export const setMoveRankSettings: Backend["setMoveRankSettings"] =
  (settings) => command("setMoveRankSettings", [settings]);
// Custom-bound replies come from the script or setStrengthEstimate too: the mock
// must not normalize bounds, split samples or calculate estimates in TypeScript.
export const strengthEstimate: Backend["strengthEstimate"] = (bounds) =>
  query("strengthEstimate", bounds === undefined ? [] : [bounds], () => strengthView ?? {
    ...emptyStrengthEstimate(lastDocument?.revision ?? 0),
    // Nothing analysed: every move of the displayed line is still to do.
    total_moves: Math.max(0, (lastDocument?.view.graph.points.length ?? 1) - 1),
  });
// The interface sends this by itself whenever the display changes: a query, so tests need not script it.
export const setAnalysisExtras: Backend["setAnalysisExtras"] = (extras) => query("setAnalysisExtras", [extras], () => undefined);
export const setPureNetwork: Backend["setPureNetwork"] = (enabled) => command("setPureNetwork", [enabled]);
export const setPondering: Backend["setPondering"] = (enabled) => command("setPondering", [enabled]);
export const analysisLimits: Backend["analysisLimits"] = () =>
  query("analysisLimits", [], () => lastAnalysisLimits);
export const setAnalysisLimits: Backend["setAnalysisLimits"] = (settings) =>
  command("setAnalysisLimits", [settings]);
export const analysisState: Backend["analysisState"] = async () => {
  const state = await query("analysisState", [], () => lastAnalysisState);
  defaultField(state, "pure_network", false);
  return state;
};
export const togglePondering: Backend["togglePondering"] = () => command("togglePondering", []);
export const autoQuickAnalyzeOnLoad: Backend["autoQuickAnalyzeOnLoad"] = () =>
  query("autoQuickAnalyzeOnLoad", [], () => lastAutoQuickAnalyzeOnLoad);
export const setAutoQuickAnalyzeOnLoad: Backend["setAutoQuickAnalyzeOnLoad"] = (enabled) =>
  command("setAutoQuickAnalyzeOnLoad", [enabled]);
/** Unscripted, an engine is named by its model file; Rust's naming rules are not simulated. */
export const engineDisplayName: Backend["engineDisplayName"] = (profile) =>
  query("engineDisplayName", [profile], () => {
    const model = profile.canonical_model_name ?? profile.model;
    const file = model.split(/[\\/]/).pop() ?? model;
    const stem = file.replace(/\.(bin|txt)(\.gz)?$/i, "").replace(/-s\d+[KMG]?-d\d+[KMG]?$/i, "");
    return { type: "Text", data: stem || file };
  });
export const configureEngine: Backend["configureEngine"] = (profile) =>
  command("configureEngine", [profile]);
export const detectEngines: Backend["detectEngines"] = () => command("detectEngines", []);
export const autoConfigureEngine: Backend["autoConfigureEngine"] = () => command("autoConfigureEngine", []);
export const tryConfigureEngine: Backend["tryConfigureEngine"] = (profile) =>
  command("tryConfigureEngine", [profile]);
export const foxSearch: Backend["foxSearch"] = (query) => command("foxSearch", [query]);
export const foxNextPage: Backend["foxNextPage"] = (uid, cursor) => command("foxNextPage", [uid, cursor]);
export const foxOpenGame: Backend["foxOpenGame"] = (gameId) => command("foxOpenGame", [gameId]);
export const foxClearHistory: Backend["foxClearHistory"] = () => command("foxClearHistory", []);
export const tencentSearch: NonNullable<Backend["tencentSearch"]> = (query) => command("tencentSearch", [query]);
export const tencentNextPage: NonNullable<Backend["tencentNextPage"]> = (query, cursor) => command("tencentNextPage", [query, cursor]);
export const tencentOpenGame: NonNullable<Backend["tencentOpenGame"]> = (chessid) => command("tencentOpenGame", [chessid]);
export const tencentClearHistory: NonNullable<Backend["tencentClearHistory"]> = () => command("tencentClearHistory", []);
export const openUrl: NonNullable<Backend["openUrl"]> = (url) => command("openUrl", [url]);
export const cacheEnabled: Backend["cacheEnabled"] = () =>
  query("cacheEnabled", [], () => lastCacheEnabled);
export const setCacheEnabled: Backend["setCacheEnabled"] = (enabled) =>
  command("setCacheEnabled", [enabled]);
export const clearCache: Backend["clearCache"] = (all) => command("clearCache", [all]);
export const clearAnalysis: Backend["clearAnalysis"] = (all) => command("clearAnalysis", [all]);
export const settings: Backend["settings"] = () => command("settings", []);
export const copySgf: NonNullable<Backend["copySgf"]> = () => command("copySgf", []);
export const pasteSgf: NonNullable<Backend["pasteSgf"]> = () => command("pasteSgf", []);
export const clipboardLink: NonNullable<Backend["clipboardLink"]> = () => query("clipboardLink", [], () => null);
export const saveCleanSgf: NonNullable<Backend["saveCleanSgf"]> = (path, keepComments, currentBranch) => command("saveCleanSgf", [path, keepComments, currentBranch]);
// Preserve the same binary arguments as production; structuredClone records the bytes without JSON expansion.
export const savePng: NonNullable<Backend["savePng"]> = (path: string, bytes: Uint8Array) => command("savePng", [path, bytes]);
export const copyPng: NonNullable<Backend["copyPng"]> = (bytes: Uint8Array) => command("copyPng", [bytes]);
export const transformBoard: NonNullable<Backend["transformBoard"]> = (transform) => command("transformBoard", [transform]);
export const alwaysOnTop: NonNullable<Backend["alwaysOnTop"]> = () => query("alwaysOnTop", [], () => false);
export const setAlwaysOnTop: NonNullable<Backend["setAlwaysOnTop"]> = (enabled) => command("setAlwaysOnTop", [enabled]);
export const uiScale: NonNullable<Backend["uiScale"]> = () => query("uiScale", [], () => 1);
export const setUiScale: NonNullable<Backend["setUiScale"]> = (scale) => command("setUiScale", [scale]);
export const playSound: NonNullable<Backend["playSound"]> = () => query("playSound", [], () => true);
export const setPlaySound: NonNullable<Backend["setPlaySound"]> = (enabled) => command("setPlaySound", [enabled]);
export const setLanguage: NonNullable<Backend["setLanguage"]> = (language) => command("setLanguage", [language]);
export const setAutoCheckUpdates: NonNullable<Backend["setAutoCheckUpdates"]> = (enabled) => command("setAutoCheckUpdates", [enabled]);
export const checkUpdate: NonNullable<Backend["checkUpdate"]> = () => command("checkUpdate", []);
export const automaticUpdateCheck: NonNullable<Backend["automaticUpdateCheck"]> = () => query("automaticUpdateCheck", [], () => null);
export const downloadUpdate: NonNullable<Backend["downloadUpdate"]> = () => command("downloadUpdate", []);
export const cancelUpdateDownload: NonNullable<Backend["cancelUpdateDownload"]> = () => command("cancelUpdateDownload", []);
// Read-only: answered from a default unless a script sets it, so opening the window needs no script.
export const updateDownloadStatus: NonNullable<Backend["updateDownloadStatus"]> = () =>
  query("updateDownloadStatus", [], () => structuredClone(defaultUpdateDownloadStatus)).then((status) => {
    fillUpdateDownloadDefaults(status);
    return status;
  });
export const installUpdate: NonNullable<Backend["installUpdate"]> = () => command("installUpdate", []);
export const fullLogStatus: NonNullable<Backend["fullLogStatus"]> = () => query("fullLogStatus", [], () => structuredClone(defaultFullLogStatus));
export const startFullLog: NonNullable<Backend["startFullLog"]> = () => command("startFullLog", []);
export const stopFullLog: NonNullable<Backend["stopFullLog"]> = () => command("stopFullLog", []);
export const exportDiagnostics: NonNullable<Backend["exportDiagnostics"]> = () => command("exportDiagnostics", []);
export const logDirectory: NonNullable<Backend["logDirectory"]> = () => query("logDirectory", [], () => "/Users/me/Library/Logs/LizzieYzy Next 2");
export const revealInFolder: NonNullable<Backend["revealInFolder"]> = (path) => command("revealInFolder", [path]);
export const setDisplaySettings: Backend["setDisplaySettings"] = (display) =>
  command("setDisplaySettings", [display]);
export const pickOpenPath: Backend["pickOpenPath"] = () => command("pickOpenPath", []);
export const pickDirectory: Backend["pickDirectory"] = () => command("pickDirectory", []);
export const pickOpenPaths: Backend["pickOpenPaths"] = () => command("pickOpenPaths", []);
export const pickSavePath: Backend["pickSavePath"] = (defaultName?: string) =>
  command("pickSavePath", defaultName === undefined ? [] : [defaultName]);
export const pickImagePath: Backend["pickImagePath"] = (defaultName: string) => command("pickImagePath", [defaultName]);
export const pickEngineFile: Backend["pickEngineFile"] = (kind) => command("pickEngineFile", [kind]);
export const pickQrImage: Backend["pickQrImage"] = () => command("pickQrImage", []);
export const pickKeyFile: Backend["pickKeyFile"] = () => command("pickKeyFile", []);
export const quit: Backend["quit"] = () => command("quit", []);
/** The title is presentation, not a command: tests read `document.title` instead. */
export const setWindowTitle: Backend["setWindowTitle"] = async (title) => {
  document.title = title;
};

export const subscribe: Backend["subscribe"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type !== "CommentaryUpdated" && event.type !== "CoachUpdated" && event.type !== "CoachMoveRated" && event.type !== "CoachSummary" && event.type !== "MatchUpdated" && event.type !== "GtpConsoleLine" && event.type !== "GameUpdated" && event.type !== "BatchAnalysisProgress" && event.type !== "WholeGameProgress" && event.type !== "BoardSyncStatusChanged" && event.type !== "BenchmarkProgress" && event.type !== "AnalysisStateChanged" && event.type !== "OpenFailed" && event.type !== "AssetDownloadProgress" && event.type !== "UpdateDownloadProgress") listener(event);
  });
  pushEvents([]);
  return id;
};

export const subscribeOpenFailed: Backend["subscribeOpenFailed"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type === "OpenFailed") listener(event.data);
  });
  pushEvents([]);
  return id;
};

export const subscribeBatchAnalysisProgress: Backend["subscribeBatchAnalysisProgress"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => { if (event.type === "BatchAnalysisProgress") listener(event.data); });
  pushEvents([]);
  return id;
};

export const subscribeWholeGameProgress: Backend["subscribeWholeGameProgress"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type === "WholeGameProgress") listener(event.data);
  });
  pushEvents([]);
  return id;
};

export const subscribeBoardSyncStatus: Backend["subscribeBoardSyncStatus"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type === "BoardSyncStatusChanged") listener(event.data);
  });
  pushEvents([]);
  return id;
};

export const subscribeBenchmarkProgress: Backend["subscribeBenchmarkProgress"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type === "BenchmarkProgress") listener(event.data);
  });
  pushEvents([]);
  return id;
};

export const subscribeAnalysisState: Backend["subscribeAnalysisState"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type === "AnalysisStateChanged") listener(event.data);
  });
  pushEvents([]);
  return id;
};

export const subscribeAssetDownloadProgress: Backend["subscribeAssetDownloadProgress"] = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type === "AssetDownloadProgress") listener(event.data);
  });
  pushEvents([]);
  return id;
};

export const subscribeUpdateDownloadProgress: NonNullable<Backend["subscribeUpdateDownloadProgress"]> = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => {
    if (event.type === "UpdateDownloadProgress") listener(event.data);
  });
  pushEvents([]);
  return id;
};

/** Explicit UI-development fixture; these illustrative speeds are not hardware
 * measurements. Replay individual events with pushEvents to inspect each view. */
export function benchmarkDemoEvents(outcome: "Completed" | "Failed" | "Cancelled" = "Completed", additionalAdvice = true): WireAppEvent[] {
  const measurements = [
    { threads: 1, completed_positions: 4, total_positions: 4, visits_per_second: 913.4, nn_evals_per_second: 886.2 },
    { threads: 2, completed_positions: 4, total_positions: 4, visits_per_second: 1421.8, nn_evals_per_second: 1389.1 },
    { threads: 4, completed_positions: 4, total_positions: 4, visits_per_second: 1832.6, nn_evals_per_second: 1774.3 },
    { threads: 6, completed_positions: 4, total_positions: 4, visits_per_second: 1903.7, nn_evals_per_second: 1810.2 },
  ];
  const snapshots: BenchmarkStatus[] = [
    { ...idleBenchmark, phase: "Running" },
    { ...idleBenchmark, phase: "Running", progress: { threads: 1, completed_positions: 2, total_positions: 4, visits_per_second: null, nn_evals_per_second: null } },
    { ...idleBenchmark, phase: "Running", progress: measurements[1]!, measurements: measurements.slice(0, 2) },
    { ...idleBenchmark, phase: "Running", progress: { threads: 4, completed_positions: 3, total_positions: 4, visits_per_second: null, nn_evals_per_second: null }, measurements: measurements.slice(0, 2) },
    {
      phase: outcome, progress: measurements[3]!, measurements,
      last_result: outcome === "Completed" ? {
        engine_fingerprint: "mock-b11-metal-engine-fingerprint",
        recommended_threads: 4,
        visits_per_second: additionalAdvice ? 2448.9 : 1832.6,
        nn_evals_per_second: additionalAdvice ? 2370.2 : 1774.3,
        nn_max_batch_size: additionalAdvice ? 2 : null,
        num_nn_server_threads_per_model: additionalAdvice ? 2 : null,
        measured_at_unix_ms: 1793404800000,
        canonical_model_name: "kata1-tf3-b11c768-s12002M-d6304M.bin.gz",
      } : null,
      error: outcome === "Failed" ? "KataGo benchmark exited with code 1: model could not be loaded" : null,
    },
  ];
  return structuredClone(snapshots.map((data) => ({ type: "BenchmarkProgress" as const, data })));
}

/** A ready-made command script for the performance window. The recommended
 * thread count deliberately differs from the highest raw visits/s candidate.
 * Additional advice selects a separate final-setting measurement; without it,
 * the displayed speeds retain the recommended main-search measurement. */
export function benchmarkDemoScript(outcome: "Completed" | "Failed" | "Cancelled" = "Completed", additionalAdvice = true): MockScript {
  const events = benchmarkDemoEvents(outcome, additionalAdvice);
  const started = (events[0]! as Extract<WireAppEvent, { type: "BenchmarkProgress" }>).data;
  const finished = (events[events.length - 1]! as Extract<WireAppEvent, { type: "BenchmarkProgress" }>).data;
  return {
    initialEvents: [{ type: "BenchmarkProgress", data: structuredClone(idleBenchmark) }],
    commands: [
      { command: "startBenchmark", result: started, events: outcome === "Cancelled" ? events.slice(0, -1) : events },
      ...(outcome === "Cancelled" ? [{ command: "cancelBenchmark" as const, result: finished, events: events.slice(-1) }] : []),
      { command: "resetBenchmark", result: idleBenchmark, events: [{ type: "BenchmarkProgress", data: idleBenchmark }] },
    ],
  };
}

export const unsubscribe: Backend["unsubscribe"] = async (id) => {
  listeners.delete(id);
};

export interface MockControls {
  setScript: typeof setScript;
  pushEvents: typeof pushEvents;
  recordedCalls: typeof recordedCalls;
  setProblemMoves: typeof setProblemMoves;
  setStrengthEstimate: typeof setStrengthEstimate;
  benchmarkDemoEvents: typeof benchmarkDemoEvents;
  benchmarkDemoScript: typeof benchmarkDemoScript;
}

declare global {
  interface Window {
    /** Present only in the e2e build, where the Vite alias swaps in this module. */
    __mockBackend?: MockControls;
    /** Set by Playwright's addInitScript so commands sent during start-up are scripted too. */
    __mockInitialScript?: MockScript;
  }
}

if (typeof window !== "undefined") {
  window.__mockBackend = { setScript, pushEvents, recordedCalls, setProblemMoves, setStrengthEstimate, benchmarkDemoEvents, benchmarkDemoScript };
  if (window.__mockInitialScript) setScript(window.__mockInitialScript);
}

export const startGame: NonNullable<Backend["startGame"]> = (setup) => command("startGame", [setup]);
export const pauseGame: NonNullable<Backend["pauseGame"]> = () => command("pauseGame", []);
export const resumeGame: NonNullable<Backend["resumeGame"]> = () => command("resumeGame", []);
export const stopGame: NonNullable<Backend["stopGame"]> = () => command("stopGame", []);
export const resignGame: NonNullable<Backend["resignGame"]> = () => command("resignGame", []);
export const takeBack: NonNullable<Backend["takeBack"]> = () => command("takeBack", []);
export const continueGame: NonNullable<Backend["continueGame"]> = (aiColor = null) => command("continueGame", [aiColor]);
export const setAiPlayer: NonNullable<Backend["setAiPlayer"]> = (color, player) => command("setAiPlayer", [color, player]);
export const playBestMove: NonNullable<Backend["playBestMove"]> = (pv = null) => command("playBestMove", [pv]);
export const gameView: NonNullable<Backend["gameView"]> = () => query("gameView", [], () => lastGameView);
export const subscribeGameUpdated: NonNullable<Backend["subscribeGameUpdated"]> = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => { if (event.type === "GameUpdated") listener(event.data); });
  pushEvents([]);
  return id;
};

export const addGtpEngine: NonNullable<Backend["addGtpEngine"]> = (entry) => command("addGtpEngine", [entry]);
export const updateGtpEngine: NonNullable<Backend["updateGtpEngine"]> = (entry) => command("updateGtpEngine", [entry]);
export const removeGtpEngine: NonNullable<Backend["removeGtpEngine"]> = (id) => command("removeGtpEngine", [id]);
export const testGtpEngine: NonNullable<Backend["testGtpEngine"]> = (id) => command("testGtpEngine", [id]);
export const gtpConsole: NonNullable<Backend["gtpConsole"]> = (engineId) => command("gtpConsole", [engineId]);
export const gtpConsoleSend: NonNullable<Backend["gtpConsoleSend"]> = (engineId, text) => command("gtpConsoleSend", [engineId, text]);
export const startMatch: NonNullable<Backend["startMatch"]> = (setup) => command("startMatch", [setup]);
export const pauseMatch: NonNullable<Backend["pauseMatch"]> = () => command("pauseMatch", []);
export const resumeMatch: NonNullable<Backend["resumeMatch"]> = () => command("resumeMatch", []);
export const stopMatch: NonNullable<Backend["stopMatch"]> = () => command("stopMatch", []);
export const setMatchGames: NonNullable<Backend["setMatchGames"]> = (n) => command("setMatchGames", [n]);
export const matchPlayNow: NonNullable<Backend["matchPlayNow"]> = () => command("matchPlayNow", []);
export const matchResign: NonNullable<Backend["matchResign"]> = (color) => command("matchResign", [color]);
export const setMatchSideParams: NonNullable<Backend["setMatchSideParams"]> = (side, wrn, pda) => command("setMatchSideParams", [side, wrn, pda]);
export const matchView: NonNullable<Backend["matchView"]> = () => command("matchView", []);
export const subscribeMatchUpdated: NonNullable<Backend["subscribeMatchUpdated"]> = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => { if (event.type === "MatchUpdated") listener(event.data); });
  pushEvents([]);
  return id;
};

export const subscribeGtpConsole: NonNullable<Backend["subscribeGtpConsole"]> = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => { if (event.type === "GtpConsoleLine") listener(event.data); });
  pushEvents([]);
  return id;
};

export const startCoach: NonNullable<Backend["startCoach"]> = (setup) => command("startCoach", [setup]);
export const coachView: NonNullable<Backend["coachView"]> = () => command("coachView", []);
export const coachHint: NonNullable<Backend["coachHint"]> = () => command("coachHint", []);
export const coachDecision: NonNullable<Backend["coachDecision"]> = (decision) => command("coachDecision", [decision]);
export const coachReview: NonNullable<Backend["coachReview"]> = () => command("coachReview", []);
export const humanslModelStatus: NonNullable<Backend["humanslModelStatus"]> = () => command("humanslModelStatus", []);
export const importHumanslModel: NonNullable<Backend["importHumanslModel"]> = (path) => command("importHumanslModel", [path]);
export const pauseAssetDownload: NonNullable<Backend["pauseAssetDownload"]> = () => command("pauseAssetDownload", []);
export const subscribeCoachUpdated: NonNullable<Backend["subscribeCoachUpdated"]> = async (listener) => {
  const id = ++nextListenerId; listeners.set(id, (event) => { if (event.type === "CoachUpdated") listener(event.data); });
  pushEvents([]); return id;
};
export const subscribeCoachMoveRated: NonNullable<Backend["subscribeCoachMoveRated"]> = async (listener) => {
  const id = ++nextListenerId; listeners.set(id, (event) => { if (event.type === "CoachMoveRated") listener(event.data); });
  pushEvents([]); return id;
};
export const subscribeCoachSummary: NonNullable<Backend["subscribeCoachSummary"]> = async (listener) => {
  const id = ++nextListenerId; listeners.set(id, (event) => { if (event.type === "CoachSummary") listener(event.data); });
  pushEvents([]); return id;
};

export const startCommentary: NonNullable<Backend["startCommentary"]> = (request) => command("startCommentary", [request]);
export const commentaryStatus: NonNullable<Backend["commentaryStatus"]> = () => command("commentaryStatus", []);
export const stopCommentary: NonNullable<Backend["stopCommentary"]> = () => command("stopCommentary", []);
export const commentaryModels: NonNullable<Backend["commentaryModels"]> = () => command("commentaryModels", []);
export const setCommentaryKey: NonNullable<Backend["setCommentaryKey"]> = (key, remember) => command("setCommentaryKey", [key, remember]);
export const clearCommentaryKey: NonNullable<Backend["clearCommentaryKey"]> = () => command("clearCommentaryKey", []);
export const commentaryKeyStatus: NonNullable<Backend["commentaryKeyStatus"]> = () => command("commentaryKeyStatus", []);
export const setCommentarySettings: NonNullable<Backend["setCommentarySettings"]> = (settings) => command("setCommentarySettings", [settings]);
export const subscribeCommentaryUpdated: NonNullable<Backend["subscribeCommentaryUpdated"]> = async (listener) => {
  const id = ++nextListenerId;
  listeners.set(id, (event) => { if (event.type === "CommentaryUpdated") listener(event.data); });
  pushEvents([]);
  return id;
};
