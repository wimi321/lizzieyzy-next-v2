#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn profile(model: &str, canonical: Option<&str>) -> EngineProfile {
    EngineProfile {
        program: "unused-katago".into(),
        model: model.into(),
        config: "unused-config".into(),
        extra_args: Vec::new(),
        canonical_model_name: canonical.map(str::to_owned),
    }
}

#[test]
fn java_weight_display_samples_keep_aliases_and_hide_training_hashes() {
    for (model, expected) in [
        (
            "kata1-zhizi-b28c512nbt-s12763923712-d5805955894-muonfd2.bin.gz",
            "zhizi 28B muonfd2",
        ),
        ("kata1-zhizi-b40c768nbt-fdx6d.bin.gz", "zhizi 40B"),
        ("kata1-b28c512nbt-s12763923712-d5805955894.bin.gz", "28B"),
        (
            "kata1-b28c512nbt-s13255194368-d5935380940.bin.gz",
            "28B 2026-06",
        ),
        (
            "kata1-tf3-b11c768-s11500M-d6163M.bin.gz",
            "B11 · s11500M-d6163M",
        ),
        (
            "kata1-tf3-b11c768-s11750M-d6216M.bin.gz",
            "B11 · s11750M-d6216M",
        ),
    ] {
        assert_eq!(
            engine_display_name(&profile(model, None)),
            EngineDisplayName::Text(expected.into()),
            "{model}"
        );
    }
}

#[test]
fn legacy_flagship_uses_localized_catalog_tier_for_canonical_names_and_filenames() {
    for model in [
        "b11c768h12nbt3tflrs-fson-silu",
        "weights/b11c768h12nbt3tflrs-fson-silu.bin.gz",
        "C:\\weights\\B11C768H12NBT3TFLRS-FSON-SILU.BIN.GZ",
    ] {
        for engine in [profile(model, None), profile("default.bin.gz", Some(model))] {
            assert_eq!(
                engine_display_name(&engine),
                EngineDisplayName::Catalog("b11-flagship".into()),
                "{model}"
            );
        }
    }
}

#[test]
fn all_three_catalog_models_work_as_canonical_names_and_as_filenames() {
    for (model, expected) in [
        (
            "kata1-tf3-b11c768-s12002M-d6304M.bin.gz",
            EngineDisplayName::Text("Transformer B11 · 2026-09-25".into()),
        ),
        (
            "b10c512h8nbt3tflrs-fson-silu-rsnh.bin.gz",
            EngineDisplayName::Catalog("b10-balanced".into()),
        ),
        (
            "b10c384h6nbttflrs.bin.gz",
            EngineDisplayName::Catalog("b10-lightweight".into()),
        ),
    ] {
        assert_eq!(
            engine_display_name(&profile("weights/default.bin.gz", Some(model))),
            expected
        );
        assert_eq!(
            engine_display_name(&profile(&format!("weights/{model}"), None)),
            expected
        );
        assert_eq!(
            engine_display_name(&profile(
                &format!("C:\\weights\\{}", model.to_uppercase()),
                None
            )),
            expected
        );
    }
}

#[test]
fn flagship_date_is_catalog_metadata_and_never_applied_to_old_training_runs() {
    let mut catalog = ModelCatalog::bundled();
    catalog.models.get_mut("b11-flagship").unwrap().published_at = Some("2030-01-02".into());
    assert_eq!(
        display_name("kata1-tf3-b11c768-s12002M-d6304M.bin.gz", &catalog),
        EngineDisplayName::Text("Transformer B11 · 2030-01-02".into())
    );
    assert_eq!(
        display_name("kata1-tf3-b11c768-s11500M-d6163M.bin.gz", &catalog),
        EngineDisplayName::Text("B11 · s11500M-d6163M".into())
    );
    catalog.models.get_mut("b11-flagship").unwrap().published_at = None;
    assert_eq!(
        display_name("kata1-tf3-b11c768-s12002M-d6304M.bin.gz", &catalog),
        EngineDisplayName::Text("B11 · s12002M-d6304M".into())
    );
}

#[test]
fn canonical_name_takes_precedence_but_blank_names_use_model_basename() {
    let canonical = "b10c384h6nbttflrs.bin.gz";
    assert_eq!(
        engine_display_name(&profile(
            "b10c512h8nbt3tflrs-fson-silu-rsnh.bin.gz",
            Some(canonical)
        )),
        EngineDisplayName::Catalog("b10-lightweight".into())
    );
    for canonical in [None, Some(""), Some("  ")] {
        assert_eq!(
            engine_display_name(&profile("/models/自定义模型.TXT.GZ", canonical)),
            EngineDisplayName::Text("自定义模型".into())
        );
    }
    assert_eq!(
        engine_display_name(&profile(
            "weights/default.bin.gz",
            Some("custom-trained.txt")
        )),
        EngineDisplayName::Text("custom-trained".into())
    );
}

#[test]
fn java_fallback_strips_supported_extensions_and_handles_empty_paths() {
    for extension in [".bin.gz", ".txt.gz", ".bin", ".txt", ".gz", ""] {
        assert_eq!(
            engine_display_name(&profile(&format!("/models/my model{extension}"), None)),
            EngineDisplayName::Text("my model".into())
        );
    }
    for path in ["", "  ", "/models/", "C:\\models\\"] {
        assert_eq!(
            engine_display_name(&profile(path, None)),
            EngineDisplayName::Text("KataGo".into())
        );
    }
}

#[test]
fn classic_suffix_rules_preserve_two_meaningful_tokens_and_custom_aliases() {
    for (model, expected) in [
        (
            "KATA1-My-Alias-B28C512nbt-sabcdef-d123-fdx6d-muonfd2-final-extra.BIN.GZ",
            "My-Alias 28B muonfd2 final",
        ),
        ("kata1-b28c512nbt-1234-s123-dabc", "28B"),
        ("kata1-b28c512nbt", "28B"),
        ("kata1-tf2-b11c768-s99M-d22M", "B11 · s99M-d22M"),
        ("b10c123nbtflrs-custom", "b10c123nbtflrs-custom"),
        ("kata1-bad_alias-b28c512nbt", "kata1-bad_alias-b28c512nbt"),
        ("kata1-b28c512nbt-", "kata1-b28c512nbt-"),
        ("kata1-bc512nbt", "kata1-bc512nbt"),
    ] {
        assert_eq!(
            engine_display_name(&profile(model, None)),
            EngineDisplayName::Text(expected.into()),
            "{model}"
        );
    }
}

#[test]
fn display_name_wire_format_is_a_tagged_text_or_catalog_value() {
    for name in [
        EngineDisplayName::Text("Transformer B11 · 2026-09-25".into()),
        EngineDisplayName::Catalog("b10-balanced".into()),
        EngineDisplayName::Catalog("b11-flagship".into()),
    ] {
        let json = serde_json::to_value(&name).unwrap();
        match &name {
            EngineDisplayName::Text(text) => {
                assert_eq!(json, serde_json::json!({ "type": "Text", "data": text }));
            }
            EngineDisplayName::Catalog(id) => {
                assert_eq!(json, serde_json::json!({ "type": "Catalog", "data": id }));
            }
        }
        assert_eq!(
            serde_json::from_value::<EngineDisplayName>(json).unwrap(),
            name
        );
    }
    assert_eq!(
        EngineDisplayName::decl(),
        "type EngineDisplayName = { \"type\": \"Text\", \"data\": string } | { \"type\": \"Catalog\", \"data\": string };"
    );
}
