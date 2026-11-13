use app_core::{Clipboard, ClipboardFuture, ClipboardImage};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug)]
pub struct MemoryClipboard(mpsc::UnboundedSender<Request>);
#[derive(Default, Debug)]
pub struct Contents {
    pub text: Option<String>,
    pub image: Option<ClipboardImage>,
}
#[derive(Debug)]
enum Request {
    Read(oneshot::Sender<Option<String>>),
    Text(Option<String>, oneshot::Sender<()>),
    Image(ClipboardImage, oneshot::Sender<()>),
    Contents(oneshot::Sender<Contents>),
}
impl MemoryClipboard {
    pub fn new() -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut state = Contents::default();
            while let Some(request) = receiver.recv().await {
                match request {
                    Request::Read(reply) => {
                        let _ = reply.send(state.text.clone());
                    }
                    Request::Text(text, reply) => {
                        state.text = text;
                        state.image = None;
                        let _ = reply.send(());
                    }
                    Request::Image(image, reply) => {
                        state.image = Some(image);
                        state.text = None;
                        let _ = reply.send(());
                    }
                    Request::Contents(reply) => {
                        let _ = reply.send(Contents {
                            text: state.text.clone(),
                            image: state.image.clone(),
                        });
                    }
                }
            }
        });
        Self(sender)
    }
    pub async fn set(&self, text: Option<String>) {
        let (reply, response) = oneshot::channel();
        self.0.send(Request::Text(text, reply)).unwrap();
        response.await.unwrap();
    }
    pub async fn contents(&self) -> Contents {
        let (reply, response) = oneshot::channel();
        self.0.send(Request::Contents(reply)).unwrap();
        response.await.unwrap()
    }
}
impl Clipboard for MemoryClipboard {
    fn read_text(&self) -> ClipboardFuture<'_, Option<String>> {
        Box::pin(async move {
            let (reply, response) = oneshot::channel();
            self.0.send(Request::Read(reply)).unwrap();
            Ok(response.await.unwrap())
        })
    }
    fn write_text(&self, text: String) -> ClipboardFuture<'_, ()> {
        Box::pin(async move {
            self.set(Some(text)).await;
            Ok(())
        })
    }
    fn write_image(&self, image: ClipboardImage) -> ClipboardFuture<'_, ()> {
        Box::pin(async move {
            let (reply, response) = oneshot::channel();
            self.0.send(Request::Image(image, reply)).unwrap();
            response.await.unwrap();
            Ok(())
        })
    }
}

pub fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[12, 34, 56, 78]).unwrap();
    }
    bytes
}
