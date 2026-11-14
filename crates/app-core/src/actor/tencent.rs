use super::Actor;
use crate::{AppError, tencent::Request};

impl Actor {
    pub(super) async fn tencent_request(&mut self, request: Request) {
        match request {
            Request::Client(reply) => {
                let result = match &self.config.tencent_client {
                    Some(client) => Ok(client.clone()),
                    None => crate::tencent::TencentClient::new().inspect(|client| {
                        self.config.tencent_client = Some(client.clone());
                    }),
                };
                let _ = reply.send(result.map_err(AppError::from));
            }
            Request::Remember(query, reply) => {
                let mut history = self.settings.tencent_history.clone();
                history.insert(0, query);
                crate::tencent::normalize_history(&mut history);
                let _ = reply.send(self.save_tencent_history(history).await);
            }
            Request::Clear(reply) => {
                let _ = reply.send(self.save_tencent_history(Vec::new()).await);
            }
        }
    }

    async fn save_tencent_history(
        &mut self,
        history: Vec<crate::tencent::TencentQuery>,
    ) -> Result<(), AppError> {
        if self.settings.tencent_history != history {
            let mut settings = self.settings.clone();
            settings.tencent_history = history;
            settings.save(&self.directory).await?;
            self.settings = settings;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
