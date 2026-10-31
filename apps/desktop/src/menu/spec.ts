/**
 * LizzieYzy Next's menu bar as `Menu.java` builds it by default (reference commit 993e288),
 * entry for entry. Ids are the Java variable names; `key` is the legacy string the label
 * comes from. Left out: entries cut in docs/inventory/FEATURES.md (appearance themes, Yike
 * web pages, force quit, the contribute menu), menus Java hides by default (quick links, the
 * second engine) and the AI buttons on the strip.
 */
export type MenuEntry =
  | { readonly kind: "separator" }
  | {
      readonly kind: "item" | "check";
      readonly id: string;
      /** Legacy string key; `text` is used when the label is not translated. */
      readonly key: string | null;
      readonly text?: string;
      /** Appended to the label, as Java appends current values. */
      readonly suffix?: string;
    }
  | {
      readonly kind: "submenu";
      readonly id: string;
      readonly key: string | null;
      readonly text?: string;
      readonly items: readonly MenuEntry[];
    };

export type MenuSubmenu = Extract<MenuEntry, { kind: "submenu" }>;

/** The entry's text in the current language, with any value Java appends. */
export function labelOf(entry: Exclude<MenuEntry, { kind: "separator" }>, labels: Readonly<Record<string, string>>): string {
  const base = entry.key === null ? (entry.text ?? "") : (labels[entry.key] ?? entry.key);
  return base + (entry.kind !== "submenu" && entry.suffix ? entry.suffix : "");
}

const SEP: MenuEntry = { kind: "separator" };

function item(id: string, key: string | null, suffix?: string, text?: string): MenuEntry {
  return { kind: "item", id, key, ...(suffix ? { suffix } : {}), ...(text ? { text } : {}) };
}

function check(id: string, key: string | null, suffix?: string, text?: string): MenuEntry {
  return { kind: "check", id, key, ...(suffix ? { suffix } : {}), ...(text ? { text } : {}) };
}

function sub(id: string, key: string, items: readonly MenuEntry[]): MenuSubmenu {
  return { kind: "submenu", id, key, items };
}

export const MENUS: readonly MenuSubmenu[] = [
  sub("fileMenu", "Menu.fileMenu", [
    item("newBoard", "Menu.newBoard"),
    item("open", "Menu.open"),
    sub("openRecent", "Menu.openRecent", []),
    item("openUrl", "Menu.openUrl"),
    SEP,
    item("save", "Menu.save"),
    item("saveAs", "Menu.saveAs"),
    sub("saveMore", "Menu.saveMore", [
      item("saveRaw", "Menu.saveRaw"),
      item("saveCommentRaw", "Menu.saveCommentRaw"),
      item("saveBranchRaw", "Menu.saveBranchRaw"),
      item("saveMainBoardScreen", "Menu.saveMainBoardScreen"),
      item("saveSubBoardScreen", "Menu.saveSubBoardScreen"),
      item("saveWinrate", "Menu.saveWinrate"),
    ]),
    item("saveAndLoad", "Menu.saveAndLoad"),
    SEP,
    item("copyBoardScreen", "Menu.copyBoardScreen"),
    item("copySubBoardScreen", "Menu.copySubBoardScreen"),
    item("copySgf", "Menu.copySgf"),
    item("pasteSgf", "Menu.pasteSgf"),
    SEP,
    check("loadKomi", "Menu.loadKomi"),
    SEP,
    item("exit", "Menu.exit"),
  ]),
  sub("viewMenu", "Menu.viewMenu", [
    sub("panel", "Menu.panel", [
      check("subboard", "Menu.subBoard"),
      check("winrateGraph", "Menu.winrateGraph"),
      check("commitPane", "Menu.commitPane"),
      check("variationPane", "Menu.variationPane"),
      sub("variationPaneSettings", "Menu.variationPaneSettings", [
        check("showScrollVariation", "Menu.showScrollVariation"),
        item("maxTreeWidth", "Menu.maxTreeWidth"),
        check("ignoreOutOfWidth", "Menu.ignoreOutOfWidth"),
      ]),
      check("listPane", "Menu.listPane"),
      check("informationPane", "Menu.informationPane"),
      check("statusPanel", "Menu.statusPanel"),
      check("gtpPanel", "Menu.gtpPanel"),
      check("ctrlPanel", "Menu.ctrlPanel"),
      SEP,
      item("visualizedPanelSettings", "Menu.visualizedPanelSettings"),
      SEP,
      check("independentMainBoard", "Menu.independentMainBoard"),
      check("independentMainBoard2", "Menu.independentMainBoard2"),
      check("independentSubBoard", "Menu.independentSubBoard"),
      check("hawkEye", "Menu.hawkEye"),
      check("SuggestionList", "Menu.SuggestionList"),
      item("restoreDefaultPanelSizes", "Menu.restoreDefaultPanelSizes"),
    ]),
    sub("toolBar", "Menu.toolbar", [
      sub("topToolBar", "Menu.topToolBar", [
        check("topToolBarSeparated", "Menu.topToolBarSeparated"),
        check("topToolBarCombined", "Menu.topToolBarCombined"),
        check("topToolBarInVisible", "Menu.inVisible"),
        SEP,
        check("autoWrap", "Menu.autoWrap"),
        sub("customTopToolBar", "Menu.customToolbar", [
          check("showDoubleMenuBtn", "Menu.showDoubleMenuBtn"),
          check("showBasicBtn", "Menu.showBasicBtn"),
          check("playMoveTools", "Menu.playMoveTools"),
          check("allowAvoidTools", "Menu.allowAvoidTools"),
          check("showGameController", "Menu.showGameController"),
          check("showTimeLimitController", "Menu.showTimeLimitController"),
          check("showPlayoutLimitController", "Menu.showPlayoutLimitController"),
          check("showAnalyzeController", "Menu.showAnalyzeController"),
          check("showCandidateController", "Menu.showCandidateController"),
          check("showCandidateInfoController", "Menu.showCandidateInfoController"),
          check("showRuleMenu", "Menu.showRuleMenu"),
          check("showTsumeGoMenu", "Menu.showTsumeGoMenu"),
          check("showParamMenu", "Menu.showParamMenu"),
          check("showGobanMenu", "Menu.showGobanMenu"),
          check("showSaveLoadMenu", "Menu.showSaveLoadMenu"),
        ]),
      ]),
      sub("bottomToolBar", "Menu.bottomToolBar", [
        check("bottomToolbarVisible", "Menu.visible"),
        check("bottomToolbarInVisible", "Menu.inVisible"),
        SEP,
        sub("customToolbarItem", "Menu.customToolbar", [
          check("liveButton", "BottomToolbar.liveButton"),
          check("kataEstimateButton", "BottomToolbar.kataEstimate"),
          check("batchOpen", "BottomToolbar.batchOpen"),
          check("openfile", "BottomToolbar.openfile"),
          check("savefile", "BottomToolbar.savefile"),
          check("finalScore", "BottomToolbar.finalScore"),
          check("heatMap", "BottomToolbar.heatMap"),
          check("badMoves", "BottomToolbar.badMoves"),
          check("analyzeList", "BottomToolbar.analyzeList"),
          check("refresh", "BottomToolbar.refresh"),
          check("analyse", "BottomToolbar.analyse"),
          check("tryPlay", "BottomToolbar.tryPlay"),
          check("setMainButton", "BottomToolbar.setMain"),
          check("backMain", "BottomToolbar.backMain"),
          check("clearButton", "BottomToolbar.clearButton"),
          check("deleteMove", "BottomToolbar.deleteMove"),
          check("moveRank", "BottomToolbar.moveRank"),
          check("move", "BottomToolbar.move"),
          check("coords", "BottomToolbar.coords"),
          check("autoPlay", "BottomToolbar.autoPlay"),
        ]),
      ]),
    ]),
    sub("mainBoardPos", "Menu.mainBoardPos", [
      item("leftMove", "Menu.leftMove"),
      item("rightMove", "Menu.rightMove"),
    ]),
    check("coordsMenu", "Menu.coordsMenu"),
    sub("moveMenu", "Menu.moveMenu", [
      check("noMoveNum", "Menu.noMoveNum"),
      check("lastOneMoveNum", "Menu.lastOneMoveNum"),
      check("lastFiveMoveNum", "Menu.lastFiveMoveNum"),
      check("lastTenMoveNum", "Menu.lastTenMoveNum"),
      check("allMoveNum", "Menu.allMoveNum"),
      check("anyMoveNum", "Menu.anyMoveNum"),
      check("moveNumberAlwaysFromOne", "Menu.moveNumberAlwaysFromOne"),
      SEP,
      check("moveNumberInBranchFromOne", "Menu.moveNumberInBracnhFromOne"),
      check("moveNumberInBranchContinue", "Menu.moveNumberInBracnhFromOneContinue"),
      SEP,
      check("showAllMoveNumberInBranch", "Menu.showAllMoveNumberInBranch"),
      SEP,
      check("showMoveNumberOnVariationPane", "Menu.showMoveNumberOnVariationPane"),
    ]),
    sub("moveRankMenu", "Menu.moveRankMenu", [
      check("rankNoneMove", "Menu.rankMenu.rankNoneMove"),
      check("rankLastMove", "Menu.rankMenu.rankLastMove"),
      check("setCustomMoves", "Menu.rankMenu.setCustomMoves", "3"),
      check("rankAllMove", "Menu.rankMenu.rankAllMove"),
      SEP,
      check("showMoveRankInOrigin", "Menu.showMoveRankInOrigin"),
      sub("moveRankBasisMenu", "Menu.rankMenu.evaluationMode", [
        check("rankModeAuto", "Menu.rankMenu.evaluationMode.auto"),
        check("rankModeScore", "Menu.rankMenu.evaluationMode.score"),
        check("rankModeWinrate", "Menu.rankMenu.evaluationMode.winrate"),
        SEP,
        check("rankModeCombined", "Menu.rankMenu.evaluationMode.combined"),
      ]),
    ]),
    sub("Suggestions", "Menu.Suggestions", [
      check("suggestion1", "Menu.winrate"),
      check("suggestion2", "Menu.visits"),
      check("suggestion3", "Menu.scoreLead"),
      SEP,
      check("showScoreAsDiff", "Menu.showScoreAsDiff"),
      item("customInfoOrdr", "Menu.customInfoOrdr"),
      check("setCandidatesDelay", "Menu.setCandidatesDelay"),
      SEP,
      check("showSuggestionOrder", "Menu.showSuggestionOrder"),
      check("showMaxValueReverse", "Menu.showMaxValueReverse"),
      check("showWhiteSuggestWhite", "Menu.showWhiteSuggestWhite"),
      SEP,
      check("alwaysShowBlackWinrate", "Menu.alwaysShowBlackWinrate"),
      check("showVariationOnMouse", "Menu.showVariationOnMouse"),
      check("noRefreshOnMouse", "Menu.noRefreshOnMouse"),
      item("setReplayInterval", "Menu.setReplayInterval"),
    ]),
    sub("nextMoveHint", "Menu.nextMoveHint", [
      check("nextMoveHintNone", "Menu.nextMoveHintNone"),
      check("nextMoveHintSimple", "Menu.nextMoveHintSimple"),
      check("nextMoveHintInformation", "Menu.nextMoveHintInformation"),
      item("minPlayoutsForNextMove", "Menu.minPlayoutsForNextMove", "30)"),
    ]),
    item("makeAutoPlay", "Menu.makeAutoPlay"),
    item("deletePersistFile", "Menu.deletePersistFile"),
    SEP,
    sub("mainPanelSettings", "Menu.mainPanelSettings", [
      check("largeSubBoard", "Menu.largeSubBoard"),
      check("largeWinrateGraph", "Menu.largeWinrateGraph"),
      check("appendWinrateToComment", "Menu.appendWinrateToComment"),
      check("showNameInBoard", "Menu.showNameInBoard"),
      check("showCommentConrolPane", "Menu.showCommentConrolPane"),
      check("alwaysOnTop", "Menu.alwaysOnTop"),
      SEP,
      check("showNewBoardConfirmDialog", "Menu.showNewBoardConfirmDialog"),
      check("showReplaceFileConfirmDialog", "Menu.showReplaceFileConfirmDialog"),
    ]),
    sub("winrate", "Menu.winrateGraphSettings", [
      sub("showWinRateOrScoreLeadLine", "Menu.showWinRateOrScoreLeadLine", [
        check("showWinRateLineOnly", "Menu.showWinRateOrScoreLeadLine.winrate"),
        check("showScoreLeadLineOnly", "Menu.showWinRateOrScoreLeadLine.scoreLead"),
        check("showBothLine", "Menu.showWinRateOrScoreLeadLine.both"),
      ]),
      check("showWinrateGraphFill", "Menu.showWinrateGraphFill"),
      check("showBlunderBar", "Menu.showBlunderBar"),
      check("showMouseOverWinrateGraph", "Menu.showMouseOverWinrateGraph"),
      item("initialMaxScoreLead", "Menu.initialMaxScoreLead"),
    ]),
    sub("subBoardSettings", "Menu.subBoardSettings", [
      sub("heatMapSettings", "Menu.heatMapSettings", [
        check("showHeat", "Menu.showHeat"),
        check("showHeatAfterCalc", "Menu.showHeatAfterCalc"),
        check("notShowHeat", "Menu.notShowHeat"),
      ]),
      check("subBoardShowVar", "Menu.subBoardShowVar"),
      check("subBoardShowRaw", "Menu.subBoardShowRaw"),
    ]),
    sub("kataSettings", "Menu.kataSettings", [
      sub("kataScolreLead", "Menu.kataScolreLead", [
        check("leadWithKomi", "Menu.leadWithKomi"),
        check("leadWithoutKomi", "Menu.leadJustScore"),
      ]),
      sub("kataEstimate", "Menu.kataEstimate", [
        check("kataEstimateClose", "Menu.kataEstimateClose"),
        check("kataEstimateCloseView", "Menu.kataEstimateCloseView"),
        check("kataEstimateOnMainBoard", "Menu.kataEstimateOnMainBoard"),
        check("kataEstimateOnSubBoard", "Menu.kataEstimateOnSubBoard"),
        check("kataEstimateOnBothBoard", "Menu.kataEstimateOnBothBoard"),
        check("kataEstimateSaveState", "Menu.kataEstimateSaveState"),
        SEP,
        check("kataEstimateByTransparentSmall", "Menu.kataEstimateByTransparentSmall"),
        check("kataEstimateByTransparent", "Menu.kataEstimateByTransparent"),
        check("kataEstimateByTransparentNotOnLive", "Menu.kataEstimateByTransparentNotOnLive"),
        check("kataEstimateByBigSquare", "Menu.kataEstimateByBigSquare"),
        check("kataEstimateBySize", "Menu.kataEstimateBySize"),
        sub("kataEstimateInPureNet", "Menu.kataEstimateInPureNet", [
          check("pureEstimateByTransparentSmall", "Menu.kataEstimateByTransparentSmall"),
          check("pureEstimateByTransparent", "Menu.kataEstimateByTransparent"),
          check("pureEstimateByTransparentNotOnLive", "Menu.kataEstimateByTransparentNotOnLive"),
          check("pureEstimateByBigSquare", "Menu.kataEstimateByBigSquare"),
          check("pureEstimateBySize", "Menu.kataEstimateBySize"),
        ]),
        SEP,
        check("useMovesOwnership", "Menu.useMovesOwnership"),
        check("useKataEstimateShortcut", "Menu.useKataEstimateShortcut"),
      ]),
    ]),
    SEP,
    sub("layoutModeMenu", "Menu.layoutMode", [
      item("defaultView", "Menu.defaultView"),
      item("classicView", "Menu.classicView"),
      item("minView", "Menu.minView"),
      SEP,
      check("extraMode3", "Menu.extraMode3"),
      check("extraMode1", "Menu.extraMode1"),
      check("extraMode2", "Menu.extraMode2"),
    ]),
    sub("independentBoardMode", "Menu.independentBoardMode", [
      item("indepBothBoard", "Menu.indepBothBoard"),
      item("indepMainBoard", "Menu.indepMainBoard"),
      item("indepSubBoard", "Menu.indepSubBoard"),
      item("indepExtraBoard", "Menu.indepExtraBoard"),
    ]),
    SEP,
    item("customeMode1", "Menu.customLayout", "1(Alt+8)"),
    item("customeMode2", "Menu.customLayout", "2(Alt+9)"),
  ]),
  sub("gameMenu", "Menu.game", [
    sub("newGame", "Menu.newGame", [
      item("newGenmoveGame", "Menu.newGenmoveGame"),
      item("newAnalyzeModeGame", "Menu.newAnalyzeModeGame"),
      item("newEngineGame", "Menu.newEngineGame"),
      item("newHumanSlGame", "Menu.newHumanSlGame"),
    ]),
    sub("continueGameAgainstAi", "Menu.continueGameAgainstAi", [
      item("continueGenmoveGameAsWhite", "Menu.continueGenmoveGameAsWhite"),
      item("continueGenmoveGameAsBlack", "Menu.continueGenmoveGameAsBlack"),
      item("continueAnalyzeGameAsWhite", "Menu.continueAnalyzeGameAsWhite"),
      item("continueAnalyzeGameAsBlack", "Menu.continueAnalyzeGameAsBlack"),
    ]),
    check("scoreGame", "Menu.scoreGame"),
    SEP,
    item("setAiTime", "Menu.setAiTime"),
    SEP,
    item("breakGame", "Menu.breakGame"),
    item("breakEngineGame", "Menu.breakEngineGame"),
    item("pauseEngineGame", "Menu.pauseEngineGame"),
    item("changeEngineGameNumbers", "Menu.changeEngineGameNumbers"),
    item("intervention", "Menu.intervention"),
    SEP,
    item("playBestMove", "Menu.playBestMove"),
    item("playPassMove", "Menu.playPassMove"),
    sub("startingPositionSetup", "Menu.startingPositionSetup", [
      check("setupModeToggle", "Menu.setupModeToggle"),
      check("setupToolBlack", "Menu.setupToolBlack"),
      check("setupToolWhite", "Menu.setupToolWhite"),
      check("setupToolErase", "Menu.setupToolErase"),
      SEP,
      item("setupClearAll", "Menu.setupClearAll"),
      SEP,
      item("setupBlackToPlay", "Menu.setupBlackToPlay"),
      item("setupWhiteToPlay", "Menu.setupWhiteToPlay"),
      SEP,
      item("convertCurrentPosition", "Menu.convertCurrentPositionToStartingPosition"),
    ]),
    item("continueLadder", "Menu.game.continueLadder"),
  ]),
  sub("analyzeMenu", "Menu.analyze", [
    item("togglePonder", "Menu.togglePonder"),
    item("aiCommentary", "Menu.aiCommentary"),
    SEP,
    item("hawkEye2", "Menu.hawkEye"),
    item("tsumeGoMenu", "Menu.tsumeGo"),
    item("captureTsumeGo", "Menu.captureTsumeGo"),
    SEP,
    item("autoAnalyze", "Menu.autoAnalyze"),
    item("batchAnalyze", "Menu.batchAnalyze"),
    item("batchAnalysisMode", "Menu.batchAnalysisMode"),
    SEP,
    item("stopAutoAnalyze", "Menu.stopAutoAnalyze"),
    item("batchAnalyzeTable", "Menu.batchAnalyzeTable"),
    SEP,
    item("wholeGameDeepAnalysis", "Menu.wholeGameDeepAnalysis"),
    SEP,
    item("flashAnalyzeAllGame", "Menu.flashAnalyzeAllGame"),
    item("flashAnalyzePartGame", "Menu.flashAnalyzePartGame"),
    item("flashAnalyzeAllBranches", "Menu.flashAnalyzeAllBranches"),
    item("flashAnalyzeSettings", "Menu.flashAnalyzeSettings"),
    item("playerStrengthEstimate", "Menu.playerStrengthEstimate"),
    SEP,
    item("showHeatmap", "Menu.showHeatmap"),
    check("showPolicy", "Menu.showPolicy"),
    SEP,
    item("clearAllLizzieCache", "Menu.clearAllLizzieCache"),
    item("clearThisLizzieCache", "Menu.clearThisLizzieCache"),
    item("clearAllLizzieBestmoves", "Menu.clearAllLizzieBestmoves"),
    item("clearThisLizzieBestmoves", "Menu.clearThisLizzieBestmoves"),
  ]),
  sub("editMenu", "Menu.edit", [
    item("addBlack", "Menu.addBlack"),
    item("addWhite", "Menu.addWhite"),
    item("alternatelyMoves", "Menu.alternatelyMoves"),
    check("allowDoubleClick", "Menu.allowDoubleClick"),
    check("allowDrag", "Menu.allowDrag"),
    check("allowClickReview", "Menu.allowClickReview"),
    SEP,
    check("insertBlack", "Menu.insertBlack"),
    check("insertWhite", "Menu.insertWhite"),
    SEP,
    item("clearBoard", "Menu.clearBoard"),
    item("backToMainBranch", "Menu.backToMainBranch"),
    item("setAsMain", "Menu.setAsMain"),
    SEP,
    item("jumpToFirst", "Menu.jumpToFirst"),
    item("jumpToLast", "Menu.jumpToLast"),
    item("jumpToLeft", "Menu.jumpToLeft"),
    item("jumpToRight", "Menu.jumpToRight"),
    SEP,
    item("delete", "Menu.delete"),
    item("deleteBranch", "Menu.deleteBranch"),
    item("undoDelete", "Menu.undoDelete"),
    item("redoDelete", "Menu.redoDelete"),
    SEP,
    item("setInfo", "Menu.setInfo"),
    item("setBoard", "Menu.setBoard"),
    SEP,
    item("exchange", "Menu.exchange"),
    item("spinRight", "Menu.spinRight"),
    item("spinLeft", "Menu.spinLeft"),
    item("mirrorVertical", "Menu.mirrorVertical"),
    item("mirrorHorizon", "Menu.mirrorHorizon"),
  ]),
  sub("live", "Menu.sync", [
    item("yikeLive", "Menu.yikeLive"),
    item("foxAi", "Fox.title"),
    item("foxKifu", "Menu.foxKifu"),
    item("tencentKifu", "Menu.tencentKifu"),
    item("readBoard", "Menu.readBoard"),
    SEP,
    sub("webBoardMenu", "Menu.webBoard", [
      item("webBoardToggle", "Menu.webBoardStart"),
      item("webBoardCopyUrl", "Menu.webBoardCopyUrl"),
      item("webBoardExitTrial", "Menu.webBoardForceExitTrial"),
    ]),
    SEP,
    check("EnableEnterYikeGame", "Menu.EnableEnterYikeGame"),
    check("alwaysGoToLastMove", "Menu.alwaysGoToLastMove"),
    sub("readBoardSettings", "Menu.readBoardSettings", [
      check("alwaysKeepBoardStatSync", "Menu.alwaysKeepBoardStatSync"),
      check("readBoardGetFocus", "Menu.readBoardGetFocus"),
    ]),
  ]),
  sub("helpMenu", "Menu.help", [
    item("diagnosticsAndLogs", "Menu.diagnosticsAndLogs"),
    item("stopFullTrace", "Menu.stopFullTrace"),
    SEP,
    item("about", "Menu.about"),
    item("checkUpdate", "Menu.checkUpdate"),
    SEP,
    item("clearUserData", "Menu.clearAllPersonalData"),
  ]),
  sub("settings", "Menu.settings", [
    item("engineConfig", "Menu.engineConfig"),
    item("remoteCompute", "Menu.remoteCompute"),
    item("engineRules", "Menu.engineRules"),
    item("engineParameters", "Menu.engineParameters"),
    SEP,
    item("initSettings", "Menu.initSettings"),
    item("autoSetup", "Menu.autoSetup"),
    item("comprehensiveSettings", "Menu.comprehensiveSettings"),
    SEP,
    sub("language", "menu.language", [
      check("languageDefault", "menu.language.default"),
      check("languageZH", null, undefined, "简体中文"),
      check("languageTW", null, undefined, "繁體中文"),
      check("languageEN", null, undefined, "English"),
      check("languageJP", null, undefined, "日本語"),
      check("languageKR", null, undefined, "한국어"),
      check("languageTH", null, undefined, "ไทย"),
    ]),
    sub("frameFontSize", "menu.frameFontSize", [
      check("frameFontSizeSmall", "menu.frameFontSizeSmall"),
      check("frameFontSizeMiddle", "menu.frameFontSizeMiddle"),
      check("frameFontSizeBig", "menu.frameFontSizeBig"),
      check("frameFontSizeOther", "menu.frameFontSizeOther"),
    ]),
    SEP,
    check("playSound", "Menu.playSound"),
    check("notPlaySoundInSync", "Menu.notPlaySoundInSync"),
  ]),
];

/** Entries Java adds on Windows only: readboard, the board synchronization tool, is Windows software. */
const WINDOWS_ONLY: ReadonlySet<string> = new Set(["readBoard"]);

function without(menu: MenuSubmenu, ids: ReadonlySet<string>): MenuSubmenu {
  const items = menu.items
    .filter((entry) => entry.kind === "separator" || !ids.has(entry.id))
    .map((entry) => (entry.kind === "submenu" ? without(entry, ids) : entry));
  return { ...menu, items };
}

/** The menus as Java builds them on this platform. */
export function menusFor(windows: boolean): readonly MenuSubmenu[] {
  return windows ? MENUS : MENUS.map((menu) => without(menu, WINDOWS_ONLY));
}

/**
 * The engine menu follows the others. Java titles it with the current engine's name and
 * lists the configured engines ("[1] name") above the restart and shutdown entries.
 */
export function engineMenu(engine: string | null): MenuSubmenu {
  const close = sub("shutdownEngine", "Menu.shutdownEngine", [
    item("shutdownCurrentEngine", "Menu.shutdownCurrentEngine"),
    item("shutdownOtherEngine", "Menu.shutdownOtherEngine"),
    item("shutdownAllEngine", "Menu.shutdownAllEngine"),
  ]);
  const actions = [item("restartCurrentEngine", "Menu.restartCurrentEngine"), close];
  if (engine === null) return sub("engineMenu", "Menu.noEngine", [SEP, ...actions]);
  return {
    kind: "submenu",
    id: "engineMenu",
    key: null,
    text: engine,
    items: [check("engine0", null, undefined, `[1] ${engine}`), SEP, ...actions],
  };
}
