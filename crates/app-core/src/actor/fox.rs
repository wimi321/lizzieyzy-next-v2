use super::Actor;
use crate::{AppError, fox::Request};

impl Actor {
    pub(super) async fn fox_request(&mut self, request: Request) {
        match request {
            Request::Client(reply) => {
                let result = match &self.config.fox_client {
                    Some(client) => Ok(client.clone()),
                    None => crate::fox::FoxClient::new().inspect(|client| {
                        self.config.fox_client = Some(client.clone());
                    }),
                };
                let _ = reply.send(result.map_err(AppError::from));
            }
            Request::Remember(account, reply) => {
                let mut history = self.settings.fox_history.clone();
                history.insert(0, account);
                crate::fox::normalize_history(&mut history);
                let _ = reply.send(self.save_fox_history(history).await);
            }
            Request::Clear(reply) => {
                let _ = reply.send(self.save_fox_history(Vec::new()).await);
            }
        }
    }

    async fn save_fox_history(
        &mut self,
        history: Vec<crate::fox::FoxAccount>,
    ) -> Result<(), AppError> {
        if self.settings.fox_history != history {
            let mut settings = self.settings.clone();
            settings.fox_history = history;
            settings.save(&self.directory).await?;
            self.settings = settings;
        }
        Ok(())
    }
}
