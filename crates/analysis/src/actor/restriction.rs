use super::Actor;
use crate::{AnalysisError, AnalysisRestriction, request};

impl Actor {
    pub(super) async fn set_restriction(
        &mut self,
        restriction: Option<AnalysisRestriction>,
    ) -> Result<(), AnalysisError> {
        if let Some(restriction) = &restriction {
            restriction.validate()?;
            if let Some(focus) = &self.focus {
                let mut request = request::build(&focus.position, &self.config.live)?;
                restriction.apply(&mut request, &focus.position)?;
            }
        }
        if self.config.restriction == restriction {
            return Ok(());
        }
        self.cancel().await?;
        self.config.restriction = restriction;
        self.output.clear();
        self.throttle.reset();
        self.last_published = None;
        self.ordinary_display = None;
        self.display_floor = None;
        self.extras_attempted = false;
        self.live_finished = false;
        self.limited = None;
        self.limited_on_current = false;
        self.playout_exempt = false;
        // Never clear ordinary LRU entries: removing the filter restores them.
        self.offer_cached();
        self.start().await
    }
}
