use crate::WebBoardError;
use crate::actor::{Actor, Command, Input};

impl Actor {
    /// A remote trial owns the displayed temporary document. Local commands
    /// cannot move its cursor or change its tree until the host ends the trial.
    pub(in crate::actor) fn web_trial_guard(&self, command: Command) -> Option<Command> {
        if self.web_board.trial.is_none() {
            return Some(command);
        }
        macro_rules! reject {
            ($reply:expr) => {{
                let _ = $reply.send(Err(WebBoardError::TrialActive.into()));
                None
            }};
        }
        match command {
            Command::Apply(input, reply) if blocked_input(&input) => reject!(reply),
            Command::PlayBestMove(_, reply) => reject!(reply),
            Command::Game(_, reply) => reject!(reply),
            Command::Match(_, reply) => reject!(reply),
            Command::SetAiPlayer(_, _, reply) => reject!(reply),
            Command::Review(_, reply) => reject!(reply),
            Command::Batch(_, reply) => reject!(reply),
            Command::BoardSync(crate::board_sync::Request::Start(_, _), reply) => reject!(reply),
            Command::Benchmark(
                crate::benchmark::Request::Start | crate::benchmark::Request::Reset,
                reply,
            ) => reject!(reply),
            Command::EnableAcceleration(_, reply) => reject!(reply),
            Command::SetEngineSettings(_, reply) => reject!(reply),
            Command::ReplaceMainEngine(_, _, _, reply) | Command::Engines(_, reply) => {
                reject!(reply)
            }
            Command::TryConfigureEngine(_, _, reply) => reject!(reply),
            Command::BeginAutoSetup(reply) => reject!(reply),
            Command::TryAutoEngine(_, reply) => reject!(reply),
            Command::JavaImport(crate::java_import::Request::Confirm(_, reply)) => reject!(reply),
            Command::Remote(
                crate::remote::Request::Add(_, _, reply)
                | crate::remote::Request::Update(_, _, _, reply)
                | crate::remote::Request::Local(reply),
            ) => reject!(reply),
            Command::ZhiziCredentialsChanged(reply) => reject!(reply),
            Command::Commentary(crate::commentary::Request::Start(_, reply)) => reject!(reply),
            Command::Coach(crate::coach_api::Request::Start(_, reply)) => reject!(reply),
            Command::Coach(
                crate::coach_api::Request::Hint(reply)
                | crate::coach_api::Request::Decision(_, reply),
            ) => reject!(reply),
            Command::Coach(crate::coach_api::Request::Review(reply)) => reject!(reply),
            Command::Scoring(
                crate::scoring::Request::Start(reply)
                | crate::scoring::Request::Toggle(_, reply)
                | crate::scoring::Request::Rule(_, reply),
            ) => reject!(reply),
            Command::Scoring(crate::scoring::Request::Commit(reply)) => reject!(reply),
            Command::Scoring(crate::scoring::Request::Cancel(reply)) => reject!(reply),
            Command::AnalysisTools(
                crate::analysis_tools::Request::Set(_, _, reply)
                | crate::analysis_tools::Request::Add(_, _, reply)
                | crate::analysis_tools::Request::Remove(_, reply),
            ) => reject!(reply),
            Command::AnalysisTools(
                crate::analysis_tools::Request::Clear(reply)
                | crate::analysis_tools::Request::Depth(_, _, reply),
            ) => reject!(reply),
            Command::AnalysisTools(crate::analysis_tools::Request::Search(_, reply)) => {
                reject!(reply)
            }
            Command::AnalysisTools(crate::analysis_tools::Request::Ladder(reply)) => reject!(reply),
            Command::SaveSlots(
                crate::save_slots_api::Request::Create(_, reply)
                | crate::save_slots_api::Request::Overwrite(_, reply),
            ) => reject!(reply),
            command => Some(command),
        }
    }
}

fn blocked_input(input: &Input) -> bool {
    matches!(
        input,
        Input::Edit(_)
            | Input::OpenBytes(_)
            | Input::OpenPath(_)
            | Input::LoadSlot(_)
            | Input::OpenDroppedPaths(_)
            | Input::Save(_)
            | Input::SaveCleanSgf(_, _, _)
            | Input::NewGame(_)
            | Input::FinishCapture(_, _)
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
            | Input::SetAnalysisExtras(_)
            | Input::SetAutoQuickAnalyzeOnLoad(_)
            | Input::EnsureNoBatch
            | Input::PrepareUpdateInstall
    )
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
