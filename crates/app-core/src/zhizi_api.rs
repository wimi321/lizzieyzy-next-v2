//! Account HTTP runs outside the document actor; only the service handle crosses it.
use crate::{AppCore, AppError, actor::Command, zhizi::*};
use tokio::sync::oneshot;

impl AppCore {
    async fn zhizi_credentials_changed(&self) -> Result<(), AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::ZhiziCredentialsChanged(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }

    async fn zhizi_service(&self) -> Result<ZhiziService, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::ZhiziService(reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)
    }

    pub async fn zhizi_status(&self) -> Result<ZhiziStatus, AppError> {
        Ok(self.zhizi_service().await?.status())
    }

    pub async fn zhizi_login_password(
        &self,
        account: String,
        password: String,
        remember: bool,
    ) -> Result<ZhiziStatus, AppError> {
        let status = self
            .zhizi_service()
            .await?
            .login_password(account, password, remember)
            .await?;
        self.zhizi_credentials_changed().await?;
        Ok(status)
    }

    pub async fn zhizi_login_code(
        &self,
        account: String,
        code: String,
        remember: bool,
    ) -> Result<ZhiziStatus, AppError> {
        let status = self
            .zhizi_service()
            .await?
            .login_code(account, code, remember)
            .await?;
        self.zhizi_credentials_changed().await?;
        Ok(status)
    }

    pub async fn zhizi_send_code(
        &self,
        account: String,
        purpose: ZhiziVerificationPurpose,
    ) -> Result<ZhiziCodeDelivery, AppError> {
        Ok(self
            .zhizi_service()
            .await?
            .send_code(account, purpose)
            .await?)
    }

    pub async fn zhizi_reset_password(
        &self,
        account: String,
        code: String,
        password: String,
        remember: bool,
    ) -> Result<ZhiziStatus, AppError> {
        let status = self
            .zhizi_service()
            .await?
            .reset_password(account, code, password, remember)
            .await?;
        self.zhizi_credentials_changed().await?;
        Ok(status)
    }

    pub async fn zhizi_logout(&self) -> Result<ZhiziStatus, AppError> {
        let status = self.zhizi_service().await?.logout().await?;
        self.zhizi_credentials_changed().await?;
        Ok(status)
    }

    pub async fn zhizi_overview(&self, force: bool) -> Result<ZhiziOverview, AppError> {
        Ok(self.zhizi_service().await?.overview(force).await?)
    }

    pub async fn zhizi_usages(&self, page: u32) -> Result<ZhiziUsagePage, AppError> {
        Ok(self.zhizi_service().await?.usages(page).await?)
    }

    pub async fn zhizi_credits(&self, page: u32) -> Result<ZhiziCreditPage, AppError> {
        Ok(self.zhizi_service().await?.credits(page).await?)
    }
}
