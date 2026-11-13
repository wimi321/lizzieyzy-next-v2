use super::*;

pub(crate) enum Input {
    OpenBytes(Vec<u8>),
    OpenPath(PathBuf),
    OpenDroppedPaths(Vec<PathBuf>),
    Save(PathBuf),
    NewGame(crate::NewGameOptions),
    ConvertToStartPosition,
    StartTrial,
    EndTrial,
    ClearRecentFiles,
    Edit(document::Command),
    Undo,
    Redo,
    SetPondering(bool),
    SetLanguage(crate::UiLanguage),
    SetAutoCheckUpdates(bool),
    SetCacheEnabled(bool),
    ClearCache(bool),
    ClearAnalysis(bool),
    SetAutoQuickAnalyzeOnLoad(bool),
    ConfigureEngine(Option<EngineProfile>),
    GenerateTsumego(crate::TsumegoOptions),
    ExitTsumego,
    PrepareUpdateInstall,
    RecoverUpdateInstall,
    Shutdown,
}

pub(crate) enum Command {
    ClaimUpdateCheck(u64, oneshot::Sender<Result<bool, AppError>>),
    Support(oneshot::Sender<Result<crate::support::SupportHandle, AppError>>),
    Updates(oneshot::Sender<Result<crate::updates::UpdateHandle, AppError>>),
    UpdateStatus(oneshot::Sender<Result<crate::UpdateDownloadStatus, AppError>>),
    AttachUpdates(
        crate::updates::UpdateHandle,
        oneshot::Sender<Result<(), AppError>>,
    ),
    Assets(oneshot::Sender<Result<crate::assets::AssetHandle, AppError>>),
    ReplaceMainEngine(
        crate::EngineEntry,
        EngineProfile,
        bool,
        oneshot::Sender<Result<Settings, AppError>>,
    ),
    DataDirectory(oneshot::Sender<PathBuf>),
    SetEngineSettings(
        crate::EngineSettings,
        oneshot::Sender<Result<crate::EngineSettings, AppError>>,
    ),
    JavaImport(crate::java_import::Request),
    Engines(
        crate::engines::Request,
        oneshot::Sender<Result<Settings, AppError>>,
    ),
    SetDisplaySettings(
        crate::DisplaySettings,
        oneshot::Sender<Result<crate::DisplaySettings, AppError>>,
    ),
    AnalysisLimit(crate::analysis_limit::Request),
    Benchmark(
        crate::benchmark::Request,
        oneshot::Sender<Result<crate::BenchmarkStatus, AppError>>,
    ),
    BoardSync(
        crate::board_sync::Request,
        oneshot::Sender<Result<crate::BoardSyncStatus, AppError>>,
    ),
    BeginAutoSetup(oneshot::Sender<Result<crate::auto_setup::Start, AppError>>),
    TryAutoEngine(
        crate::auto_setup::Trial,
        oneshot::Sender<Result<EngineProfile, AppError>>,
    ),
    TsumegoStatus(oneshot::Sender<crate::TsumegoStatus>),
    StrengthEstimate(
        Option<[i32; 2]>,
        oneshot::Sender<Result<crate::StrengthEstimateView, AppError>>,
    ),
    Fox(crate::fox::Request),
    ProblemMoves(
        crate::ProblemMovesQuery,
        oneshot::Sender<Result<crate::ProblemMovesView, AppError>>,
    ),
    SetMoveRankSettings(
        crate::MoveRankSettings,
        oneshot::Sender<Result<crate::MoveRankSettings, AppError>>,
    ),
    Review(
        crate::review::ReviewInput,
        oneshot::Sender<Result<crate::WholeGameProgress, AppError>>,
    ),
    ReviewProgress(oneshot::Sender<Option<crate::WholeGameProgress>>),
    ReviewResult(u32, oneshot::Sender<Result<Option<AnalysisView>, AppError>>),
    ReviewAnalysis(u32, oneshot::Sender<Option<crate::StoredAnalysis>>),
    VariationTree(oneshot::Sender<VariationTreeView>),
    StoneOrigin(
        go_core::Point,
        oneshot::Sender<Result<Option<crate::StoneOriginView>, AppError>>,
    ),
    PreviewVariation(
        u32,
        Vec<go_core::Move>,
        oneshot::Sender<Result<VariationPreviewView, AppError>>,
    ),
    Apply(Input, oneshot::Sender<Result<u64, AppError>>),
    Settings(oneshot::Sender<Settings>),
    TryConfigureEngine(
        crate::EngineSetupProfile,
        PathBuf,
        oneshot::Sender<Result<(), AppError>>,
    ),
}
