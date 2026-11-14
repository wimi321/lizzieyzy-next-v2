use super::{CommentaryError, CommentaryPhase, CommentaryUpdated, Connection};
use crate::commentary_client::CommentaryMessage;
use ::commentary::{Evidence, Language, Message};
use tokio::sync::watch;

pub(crate) struct Generation {
    pub connection: Connection,
    pub evidence: Evidence,
    pub messages: Vec<Message>,
    pub language: Language,
    pub view: CommentaryUpdated,
}

pub(crate) async fn generate(mut job: Generation, updates: watch::Sender<CommentaryUpdated>) {
    let result = run(&mut job, &updates).await;
    if let Err(error) = result {
        job.view.phase = CommentaryPhase::Failed;
        job.view.error = Some(error);
    } else {
        job.view.phase = CommentaryPhase::Completed;
    }
    job.view.delta.clear();
    updates.send_replace(job.view);
}

async fn run(
    job: &mut Generation,
    updates: &watch::Sender<CommentaryUpdated>,
) -> Result<(), CommentaryError> {
    let mut messages = job.messages.clone();
    for attempt in 0..2 {
        job.view.phase = CommentaryPhase::Generating;
        job.view.text.clear();
        job.view.delta.clear();
        job.view.replacement = attempt > 0;
        updates.send_replace(job.view.clone());
        let input: Vec<_> = messages
            .iter()
            .map(|m| CommentaryMessage {
                role: m.role.into(),
                content: m.content.clone(),
            })
            .collect();
        let answer = job
            .connection
            .client
            .complete(
                &job.connection.key,
                &job.connection.model,
                &input,
                |delta| {
                    job.view.delta.clone_from(&delta);
                    job.view.text.push_str(&delta);
                    updates.send_replace(job.view.clone());
                },
            )
            .await?;
        job.view.text = answer;
        job.view.delta.clear();
        job.view.phase = CommentaryPhase::Verifying;
        job.view.issues = ::commentary::verify(&job.view.text, &job.evidence);
        updates.send_replace(job.view.clone());
        if job.view.issues.is_empty() || attempt == 1 {
            return Ok(());
        }
        messages = ::commentary::build_correction(
            &job.messages,
            &job.view.text,
            &job.view.issues,
            job.language,
        )
        .map_err(super::domain_error)?;
    }
    Ok(())
}
