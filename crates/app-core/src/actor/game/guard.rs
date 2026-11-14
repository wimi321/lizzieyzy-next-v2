use super::*;
use crate::actor::{Command, Input};
impl Actor {
    pub(in crate::actor) fn game_guard(&self, command: Command) -> Option<Command> {
        if !self.game_playing() {
            return Some(command);
        }
        macro_rules! reject {
            ($reply:expr) => {{
                let _ = $reply.send(Err(AppError::GameRunning));
                None
            }};
        }
        match command {
            Command::Apply(input, reply) if blocked(&input) => reject!(reply),
            Command::PlayBestMove(_, reply) => reject!(reply),
            Command::Review(_, reply) => reject!(reply),
            Command::Batch(_, reply) => reject!(reply),
            Command::SetEngineSettings(_, reply) => reject!(reply),
            Command::ReplaceMainEngine(_, _, _, reply) | Command::Engines(_, reply) => {
                reject!(reply)
            }
            Command::TryConfigureEngine(_, _, reply) => reject!(reply),
            Command::BeginAutoSetup(reply) => reject!(reply),
            Command::TryAutoEngine(_, reply) => reject!(reply),
            Command::JavaImport(crate::java_import::Request::Confirm(_, reply)) => reject!(reply),
            Command::BoardSync(crate::board_sync::Request::Start(_, _), reply) => reject!(reply),
            Command::Benchmark(
                crate::benchmark::Request::Start | crate::benchmark::Request::Reset,
                reply,
            ) => reject!(reply),
            Command::AnalysisLimit(crate::analysis_limit::Request::Toggle(reply)) => reject!(reply),
            command => Some(command),
        }
    }
}
fn blocked(input: &Input) -> bool {
    match input {
        Input::Edit(
            document::Command::Play(_) | document::Command::Step(_) | document::Command::GoTo(_),
        ) => false,
        Input::Edit(_)
        | Input::OpenBytes(_)
        | Input::OpenPath(_)
        | Input::OpenDroppedPaths(_)
        | Input::NewGame(_)
        | Input::PasteSgf
        | Input::ConvertToStartPosition
        | Input::StartTrial
        | Input::EndTrial
        | Input::GenerateTsumego(_)
        | Input::ExitTsumego
        | Input::Undo
        | Input::Redo
        | Input::ClearAnalysis(_)
        | Input::ConfigureEngine(_)
        | Input::SetPureNetwork(_)
        | Input::SetPondering(_)
        | Input::EnsureNoBatch
        | Input::PrepareUpdateInstall => true,
        _ => false,
    }
}
