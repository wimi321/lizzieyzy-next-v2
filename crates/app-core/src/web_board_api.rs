use crate::{AppCore, AppError, WebBoardOptions, WebBoardStatus, actor};
use tokio::sync::oneshot;

pub(crate) enum Request {
    Start,
    Stop,
    Status,
    Options(WebBoardOptions),
    ForceEnd,
}

impl AppCore {
    pub async fn start_web_board(&self) -> Result<WebBoardStatus, AppError> {
        self.web_request(Request::Start).await
    }
    pub async fn stop_web_board(&self) -> Result<(), AppError> {
        self.web_request(Request::Stop).await.map(|_| ())
    }
    pub async fn web_board_status(&self) -> Result<WebBoardStatus, AppError> {
        self.web_request(Request::Status).await
    }
    pub async fn set_web_board_options(&self, options: WebBoardOptions) -> Result<(), AppError> {
        self.web_request(Request::Options(options))
            .await
            .map(|_| ())
    }
    pub async fn force_end_web_trial(&self) -> Result<(), AppError> {
        self.web_request(Request::ForceEnd).await.map(|_| ())
    }
    async fn web_request(&self, request: Request) -> Result<WebBoardStatus, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::WebBoard(request, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}
