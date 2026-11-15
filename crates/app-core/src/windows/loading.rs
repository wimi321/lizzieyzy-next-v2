use super::{WindowBounds, WindowPlacement};
use serde_json::{Value, json};

/// Only optional persisted geometry is recoverable. Invalid layout choices and
/// unrelated settings still use the normal strict settings validation.
pub(crate) fn recover_positions(settings: &mut Value) {
    let Some(display) = settings.get_mut("display").and_then(Value::as_object_mut) else {
        return;
    };
    recover_placement(display.get_mut("main_window"), true);
    for field in ["floating_main", "floating_sub"] {
        recover_bounds(display.get_mut(field));
    }
    if let Some(layouts) = display
        .get_mut("custom_layouts")
        .and_then(Value::as_array_mut)
    {
        for layout in layouts {
            let Some(layout) = layout.as_object_mut() else {
                continue;
            };
            recover_placement(layout.get_mut("main_window"), false);
            for field in ["floating_main", "floating_sub"] {
                recover_bounds(layout.get_mut(field));
            }
        }
    }
}

fn recover_placement(value: Option<&mut Value>, optional: bool) {
    let Some(value) = value else { return };
    if optional && value.is_null() {
        return;
    }
    if serde_json::from_value::<WindowPlacement>(value.clone())
        .is_ok_and(|placement| placement.bounds.validate().is_ok())
    {
        return;
    }
    tracing::warn!("invalid saved window placement ignored");
    *value = if optional {
        Value::Null
    } else {
        json!({ "bounds": { "x": 0, "y": 0, "width": 1280, "height": 820 }, "maximized": false })
    };
}

fn recover_bounds(value: Option<&mut Value>) {
    let Some(value) = value else { return };
    if value.is_null()
        || serde_json::from_value::<WindowBounds>(value.clone())
            .is_ok_and(|bounds| bounds.validate().is_ok())
    {
        return;
    }
    tracing::warn!("invalid saved floating window bounds ignored");
    *value = Value::Null;
}
