#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn actor() -> Actor {
    let (status, _) = watch::channel(GtpStatus::Ready(GtpInfo::default()));
    let (lines, _) = broadcast::channel(16);
    Actor {
        config: GtpSessionConfig::default(),
        status,
        lines,
        history: VecDeque::new(),
        sequence: 0,
        startup: VecDeque::new(),
        queued: VecDeque::new(),
        active: None,
        decoder: ::gtp::Decoder::default(),
        info: GtpInfo::default(),
        failure: None,
    }
}

type ReplyReceiver = oneshot::Receiver<Result<::gtp::Response, GtpSessionError>>;

fn pending_commands(actor: &mut Actor) -> (ReplyReceiver, ReplyReceiver) {
    let (active_reply, active) = oneshot::channel();
    actor.active = Some(Active {
        pending: Pending {
            text: "genmove b".into(),
            reply: Some(active_reply),
            startup: false,
            timeout: None,
        },
        id: None,
        deadline: Instant::now(),
    });
    let (queued_reply, queued) = oneshot::channel();
    actor.message(Message::Execute("name".into(), None, queued_reply));
    (active, queued)
}

fn assert_protocol_failure(actor: &Actor, active: &mut ReplyReceiver, queued: &mut ReplyReceiver) {
    let error = actor.failure.clone().expect("GTP protocol failure");
    assert!(matches!(error, GtpSessionError::Protocol(_)), "{error:?}");
    assert_eq!(*actor.status.borrow(), GtpStatus::Failed(error.clone()));
    assert_eq!(active.try_recv().unwrap(), Err(error.clone()));
    assert_eq!(queued.try_recv().unwrap(), Err(error));
    assert!(actor.active.is_none());
    assert!(actor.queued.is_empty());
}

#[test]
fn malformed_transport_stdout_fails_active_and_queued_commands_as_protocol_errors() {
    for reason in ["stdout line is not UTF-8", "stdout line exceeds 8 MiB"] {
        let mut actor = actor();
        let (mut active, mut queued) = pending_commands(&mut actor);
        let mut lines = actor.lines.subscribe();

        actor.output(Output::MalformedStdout(reason));

        assert_protocol_failure(&actor, &mut active, &mut queued);
        let diagnostic = lines.try_recv().unwrap();
        assert_eq!(diagnostic.direction, GtpDirection::Received);
        assert!(diagnostic.text.contains(reason), "{diagnostic:?}");
        assert_eq!(actor.history.back(), Some(&diagnostic));
        assert_eq!(actor.history.len(), 1);
    }
}

#[test]
fn malformed_text_stdout_uses_the_same_protocol_failure_path() {
    let mut actor = actor();
    let (mut active, mut queued) = pending_commands(&mut actor);

    actor.output(Output::Stdout("not a GTP response".into()));

    assert_protocol_failure(&actor, &mut active, &mut queued);
    let diagnostic = actor.history.back().unwrap();
    assert_eq!(diagnostic.direction, GtpDirection::Received);
    assert_eq!(diagnostic.text, "not a GTP response");
}

#[test]
fn cleanup_output_retains_the_original_gtp_protocol_failure() {
    let mut actor = actor();
    let (mut active, mut queued) = pending_commands(&mut actor);
    actor.output(Output::MalformedStdout("stdout line is not UTF-8"));
    assert_protocol_failure(&actor, &mut active, &mut queued);
    let original = actor.failure.clone();

    // The driver continues draining while the failed GTP actor cleans up.
    actor.output(Output::Stderr("last diagnostic".into()));
    actor.output(Output::Stdout("= D4".into()));
    actor.output(Output::Stdout(String::new()));
    actor.output(Output::MalformedStdout("stdout line exceeds 8 MiB"));
    actor.output(Output::Fault("late pipe failure".into()));

    assert_eq!(actor.failure, original);
    assert!(
        actor.history.iter().any(|line| {
            line.direction == GtpDirection::Stderr && line.text == "last diagnostic"
        })
    );
    assert!(
        actor
            .history
            .iter()
            .any(|line| { line.direction == GtpDirection::Received && line.text == "= D4" })
    );
    assert!(actor.active.is_none());
    assert!(actor.queued.is_empty());
}
