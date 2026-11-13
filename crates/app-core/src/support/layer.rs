use std::fmt::Write;

use tokio::sync::{mpsc, watch};
use tracing::{Event, Metadata, Subscriber, field::Visit, subscriber::Interest};
use tracing_subscriber::{Layer, layer::Context};

use super::{MAX_RECORD_BYTES, Message};

/// Formatting and enqueueing never perform file I/O or wait for the logging task.
/// A full queue drops new records, bounding memory even during full tracing.
#[derive(Clone)]
pub struct SupportLogLayer {
    commands: mpsc::Sender<Message>,
    verbose: watch::Receiver<bool>,
}

impl SupportLogLayer {
    pub(super) fn new(commands: mpsc::Sender<Message>, verbose: watch::Receiver<bool>) -> Self {
        Self { commands, verbose }
    }
}

impl<S: Subscriber> Layer<S> for SupportLogLayer {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        // The watch value can change without rebuilding the global callsite cache.
        Interest::sometimes()
    }

    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, S>) -> bool {
        *metadata.level() <= tracing::Level::INFO || *self.verbose.borrow()
    }

    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(tracing::level_filters::LevelFilter::TRACE)
    }

    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let metadata = event.metadata();
        let mut fields = Fields(String::new());
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = write!(
            fields,
            "{unix_ms} {} {}",
            metadata.level(),
            metadata.target()
        );
        event.record(&mut fields);
        fields.0.push('\n');
        let _ = self.commands.try_send(Message::Record {
            level: *metadata.level(),
            at: tokio::time::Instant::now(),
            text: fields.0,
        });
    }
}

struct Fields(String);

impl std::fmt::Write for Fields {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let available = MAX_RECORD_BYTES.saturating_sub(self.0.len() + 1);
        let mut end = text.len().min(available);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.0.push_str(&text[..end]);
        if end < text.len() {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self, " {}={value:?}", field.name());
    }
}
