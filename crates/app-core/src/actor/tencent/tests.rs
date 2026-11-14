#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AppError, actor::Command, tencent::Request, updates::UpdateError};

#[test]
fn installing_an_update_rejects_tencent_history_mutations() {
    let (mut actor, _events) = crate::actor::tests::actor();
    actor.updates.install_pending = true;
    for remember in [true, false] {
        let (reply, mut response) = tokio::sync::oneshot::channel();
        let request = if remember {
            Request::Remember(crate::tencent::TencentQuery("sample".into()), reply)
        } else {
            Request::Clear(reply)
        };
        assert!(actor.update_guard(Command::Tencent(request)).is_none());
        assert!(matches!(
            response.try_recv().unwrap(),
            Err(AppError::Update(UpdateError::Busy))
        ));
    }
    assert!(actor.settings.tencent_history.is_empty());
}
