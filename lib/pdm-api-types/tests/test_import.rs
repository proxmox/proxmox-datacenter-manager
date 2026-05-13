//! SectionConfig round-trip and helper tests for the subscription key pool.
//!
//! Run with: cargo test -p pdm-api-types --test test_import

use pdm_api_types::subscription::*;
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};
use proxmox_subscription::SubscriptionStatus;

#[test]
fn entry_roundtrip() {
    let mut config = SectionConfigData::<SubscriptionKeyEntry>::default();

    let entry = SubscriptionKeyEntry {
        key: "pve4b-aa11bb2233".to_string(),
        product_type: ProductType::Pve,
        level: SubscriptionLevel::Basic,
        source: SubscriptionKeySource::Manual,
        remote: Some("my-cluster".to_string()),
        node: Some("node1".to_string()),
        pending_clear: false,
        serverid: Some("AABBCCDD".to_string()),
        status: SubscriptionStatus::Active,
        next_due_date: Some("2027-06-01".to_string()),
        product_name: Some("Proxmox VE Basic".to_string()),
        check_time: Some(1700000000),
    };

    config.insert("pve4b-aa11bb2233".to_string(), entry);

    let raw = SubscriptionKeyEntry::write_section_config("test", &config).expect("write failed");
    let parsed = SubscriptionKeyEntry::parse_section_config("test", &raw).expect("parse failed");

    let back = parsed.get("pve4b-aa11bb2233").expect("key not found");
    assert_eq!(back.key, "pve4b-aa11bb2233");
    assert_eq!(back.product_type, ProductType::Pve);
    assert_eq!(back.source, SubscriptionKeySource::Manual);
    assert_eq!(back.remote.as_deref(), Some("my-cluster"));
    assert_eq!(back.node.as_deref(), Some("node1"));
    assert_eq!(back.status, SubscriptionStatus::Active);
    assert_eq!(back.next_due_date.as_deref(), Some("2027-06-01"));
}

#[test]
fn adopted_entry_roundtrip() {
    // Ensure SubscriptionKeySource::Adopted serializes to its kebab-case form `adopted` and
    // parses back to the same variant, so an in-place upgrade does not silently rewrite
    // adopted pool entries to Manual on the next save.
    let mut config = SectionConfigData::<SubscriptionKeyEntry>::default();
    config.insert(
        "pbsc-1122334455".to_string(),
        SubscriptionKeyEntry {
            key: "pbsc-1122334455".to_string(),
            product_type: ProductType::Pbs,
            source: SubscriptionKeySource::Adopted,
            remote: Some("backup-cluster".to_string()),
            node: Some("pbs-1".to_string()),
            ..Default::default()
        },
    );

    let raw = SubscriptionKeyEntry::write_section_config("test", &config).expect("write failed");
    assert!(
        raw.contains("\tsource adopted"),
        "expected kebab-case `adopted` in serialised form, got:\n{raw}",
    );
    let parsed = SubscriptionKeyEntry::parse_section_config("test", &raw).expect("parse failed");
    let back = parsed.get("pbsc-1122334455").expect("key not found");
    assert_eq!(back.source, SubscriptionKeySource::Adopted);
    assert_eq!(back.remote.as_deref(), Some("backup-cluster"));
}

#[test]
fn shadow_roundtrip() {
    let mut shadow = SectionConfigData::<SubscriptionKeyShadow>::default();

    shadow.insert(
        "pve4b-aa11bb2233".to_string(),
        SubscriptionKeyShadow {
            key: "pve4b-aa11bb2233".to_string(),
            product_type: ProductType::Pve,
            info: "dGVzdA==".to_string(),
        },
    );

    let raw = SubscriptionKeyShadow::write_section_config("test", &shadow).expect("write failed");
    let parsed = SubscriptionKeyShadow::parse_section_config("test", &raw).expect("parse failed");

    let back = parsed.get("pve4b-aa11bb2233").expect("key not found");
    assert_eq!(back.info, "dGVzdA==");
}

#[test]
fn deserialize_api_response_json() {
    // The legacy `nextduedate` / `productname` / `checktime` spellings are the shop's wire
    // format (mirrored from `proxmox_subscription::SubscriptionInfo`); a future shop-bundle
    // import path will feed exactly these into the pool. Keep the alias coverage explicit so a
    // serde rename without an accompanying alias gets caught at test time.
    let json = serde_json::json!({
        "key": "pve4b-aa11bb2233",
        "nextduedate": "2027-06-01",
        "product-type": "pve",
        "productname": "Proxmox VE Basic",
        "checktime": 1700000000,
        "serverid": "AABBCCDD",
        "status": "active"
    });

    let entry: SubscriptionKeyEntry = serde_json::from_value(json).unwrap();
    assert_eq!(entry.key, "pve4b-aa11bb2233");
    assert_eq!(entry.product_type, ProductType::Pve);
    assert_eq!(entry.status, SubscriptionStatus::Active);
    assert_eq!(entry.source, SubscriptionKeySource::Manual);
    assert_eq!(entry.next_due_date.as_deref(), Some("2027-06-01"));
    assert_eq!(entry.product_name.as_deref(), Some("Proxmox VE Basic"));
    assert_eq!(entry.check_time, Some(1700000000));
}

#[test]
fn deserialize_canonical_kebab_case_json() {
    // The canonical wire form for these fields uses the struct's `kebab-case` rename; verify
    // the renamed spelling round-trips through serde even though the field shapes share the
    // alias with the legacy form above.
    let json = serde_json::json!({
        "key": "pve4b-aa11bb2233",
        "next-due-date": "2027-06-01",
        "product-type": "pve",
        "product-name": "Proxmox VE Basic",
        "check-time": 1700000000,
        "status": "active"
    });

    let entry: SubscriptionKeyEntry = serde_json::from_value(json).unwrap();
    assert_eq!(entry.next_due_date.as_deref(), Some("2027-06-01"));
    assert_eq!(entry.product_name.as_deref(), Some("Proxmox VE Basic"));
    assert_eq!(entry.check_time, Some(1700000000));
}

#[test]
fn deserialize_without_optional_fields() {
    let json = serde_json::json!({
        "key": "pbsb-ee77ff8899",
        "product-type": "pbs",
    });

    let entry: SubscriptionKeyEntry = serde_json::from_value(json).unwrap();
    assert_eq!(entry.key, "pbsb-ee77ff8899");
    assert_eq!(entry.product_type, ProductType::Pbs);
    assert!(entry.remote.is_none());
    assert!(entry.next_due_date.is_none());
}

#[test]
fn product_type_classification() {
    let cases = [
        ("pve4b-1234567890", Some(ProductType::Pve), "pve"),
        ("pbss-abcdef0123", Some(ProductType::Pbs), "pbs"),
        ("pmgb-1234567890", Some(ProductType::Pmg), "pmg"),
        ("pomb-1234567890", Some(ProductType::Pom), "pom"),
        ("xxx-1234567890", None, ""),
        ("no-dash", None, ""),
    ];
    for (key, expected, marker) in cases {
        assert_eq!(ProductType::from_key(key), expected, "from_key({key})");
        if let Some(pt) = expected {
            assert_eq!(pt.as_section_type(), marker, "section_type for {key}");
        }
    }
}

#[test]
fn socket_count_extraction() {
    assert_eq!(socket_count_from_key("pve1c-1234567890"), Some(1));
    assert_eq!(socket_count_from_key("pve2b-1234567890"), Some(2));
    assert_eq!(socket_count_from_key("pve4s-1234567890"), Some(4));
    assert_eq!(socket_count_from_key("pve8p-1234567890"), Some(8));
    assert_eq!(socket_count_from_key("pbss-1234567890"), None);
    assert_eq!(socket_count_from_key("pvexb-1234567890"), None);
}

#[test]
fn remote_type_matching() {
    use pdm_api_types::remotes::RemoteType;

    assert!(ProductType::Pve.matches_remote_type(RemoteType::Pve));
    assert!(!ProductType::Pve.matches_remote_type(RemoteType::Pbs));
    assert!(ProductType::Pbs.matches_remote_type(RemoteType::Pbs));
    assert!(!ProductType::Pbs.matches_remote_type(RemoteType::Pve));
    // PMG and POM are reserved product types but PDM cannot manage those remotes yet.
    assert!(!ProductType::Pmg.matches_remote_type(RemoteType::Pve));
    assert!(!ProductType::Pmg.matches_remote_type(RemoteType::Pbs));
    assert!(!ProductType::Pom.matches_remote_type(RemoteType::Pbs));
}

#[test]
fn subscription_level_from_key_suffix() {
    assert_eq!(
        SubscriptionLevel::from_key(Some("pve4c-123")),
        SubscriptionLevel::Community
    );
    assert_eq!(
        SubscriptionLevel::from_key(Some("pve4b-123")),
        SubscriptionLevel::Basic
    );
    assert_eq!(
        SubscriptionLevel::from_key(Some("pve4s-123")),
        SubscriptionLevel::Standard
    );
    assert_eq!(
        SubscriptionLevel::from_key(Some("pve2p-123")),
        SubscriptionLevel::Premium
    );
    assert_eq!(
        SubscriptionLevel::from_key(Some("pbsb-123")),
        SubscriptionLevel::Basic
    );
    assert_eq!(SubscriptionLevel::from_key(None), SubscriptionLevel::None);
    assert_eq!(
        SubscriptionLevel::from_key(Some("")),
        SubscriptionLevel::None
    );
}

#[test]
fn subscription_level_display_fromstr_roundtrip() {
    for level in [
        SubscriptionLevel::None,
        SubscriptionLevel::Community,
        SubscriptionLevel::Basic,
        SubscriptionLevel::Standard,
        SubscriptionLevel::Premium,
        SubscriptionLevel::Unknown,
    ] {
        let s = format!("{level}");
        let parsed: SubscriptionLevel = s.parse().unwrap();
        assert_eq!(parsed, level, "roundtrip failed for {s}");
    }

    // Backward compatibility: legacy single-letter wire format still parses.
    for (letter, level) in [
        ("c", SubscriptionLevel::Community),
        ("b", SubscriptionLevel::Basic),
        ("s", SubscriptionLevel::Standard),
        ("p", SubscriptionLevel::Premium),
    ] {
        assert_eq!(letter.parse::<SubscriptionLevel>().unwrap(), level);
    }
}

#[test]
fn multiple_keys_different_types() {
    let mut config = SectionConfigData::<SubscriptionKeyEntry>::default();

    config.insert(
        "pve4b-aaaa111111".to_string(),
        SubscriptionKeyEntry {
            key: "pve4b-aaaa111111".to_string(),
            product_type: ProductType::Pve,
            status: SubscriptionStatus::Active,
            ..Default::default()
        },
    );
    config.insert(
        "pbss-bbbb222222".to_string(),
        SubscriptionKeyEntry {
            key: "pbss-bbbb222222".to_string(),
            product_type: ProductType::Pbs,
            status: SubscriptionStatus::Active,
            ..Default::default()
        },
    );

    let raw = SubscriptionKeyEntry::write_section_config("test", &config).unwrap();
    let parsed = SubscriptionKeyEntry::parse_section_config("test", &raw).unwrap();

    assert_eq!(
        parsed.get("pve4b-aaaa111111").unwrap().product_type,
        ProductType::Pve
    );
    assert_eq!(
        parsed.get("pbss-bbbb222222").unwrap().product_type,
        ProductType::Pbs
    );
}

#[test]
fn pick_best_pve_socket_key_edge_cases() {
    let pool = [
        ("pve1c-aaa", "pve1c-aaa"),
        ("pve2b-bbb", "pve2b-bbb"),
        ("pve4s-ccc", "pve4s-ccc"),
        ("pve8p-ddd", "pve8p-ddd"),
    ];
    let pick =
        |sockets: u32| pick_best_pve_socket_key(sockets, pool.iter().map(|(id, k)| (*id, *k)));

    // Exact match prefers the equally-sized key over a larger one.
    assert_eq!(pick(2), Some("pve2b-bbb"));

    // No exact match: fall through to the smallest key that still covers the node.
    assert_eq!(pick(3), Some("pve4s-ccc"));
    assert_eq!(pick(5), Some("pve8p-ddd"));

    // Single-socket node still picks the single-socket key (does not overprovision).
    assert_eq!(pick(1), Some("pve1c-aaa"));

    // Node larger than every key has no fit.
    assert_eq!(pick(16), None);

    // Empty candidate list is None.
    let empty: [(&str, &str); 0] = [];
    assert_eq!(
        pick_best_pve_socket_key(2, empty.iter().map(|(id, k)| (*id, *k))),
        None,
    );

    // Non-PVE keys are skipped silently.
    let mixed = [("a", "pbsc-aaaa111111"), ("b", "pve2b-bbbb222222")];
    assert_eq!(
        pick_best_pve_socket_key(1, mixed.iter().map(|(id, k)| (*id, *k))),
        Some("b"),
    );
}

#[test]
fn schema_accepts_pve_pbs_only() {
    use proxmox_schema::ApiType;
    let schema = SubscriptionKeyEntry::API_SCHEMA.unwrap_object_schema();
    let key_schema = schema
        .lookup("key")
        .expect("key property in object schema")
        .1;
    assert!(key_schema.parse_simple_value("garbage").is_err());
    assert!(key_schema.parse_simple_value("xxx-yyyyyyyyyy").is_err());
    assert!(key_schema.parse_simple_value("pve4b-1234567890").is_ok());
    assert!(key_schema.parse_simple_value("pbss-abcdef0123").is_ok());
    // PMG and POM are not driven by PDM today, so the schema rejects them; widen the regex
    // when remote-side support lands.
    assert!(key_schema.parse_simple_value("pmgb-deadbeef00").is_err());
    assert!(key_schema.parse_simple_value("pomb-deadbeef00").is_err());
}

#[test]
fn verify_serverid_helper() {
    let entry = SubscriptionKeyEntry {
        key: "pve4b-aa11bb2233".to_string(),
        product_type: ProductType::Pve,
        serverid: Some("AABBCCDD".to_string()),
        ..Default::default()
    };

    let mut info = proxmox_subscription::SubscriptionInfo::default();
    info.serverid = Some("AABBCCDD".to_string());
    assert_eq!(verify_serverid(&entry, &info).unwrap(), None);

    info.serverid = Some("DEADBEEF".to_string());
    let mismatch = verify_serverid(&entry, &info).unwrap().unwrap();
    assert_eq!(mismatch.expected, "AABBCCDD");
    assert_eq!(mismatch.actual, "DEADBEEF");

    // entry without serverid -> nothing to verify
    let entry = SubscriptionKeyEntry {
        key: "pve4b-aa11bb2233".to_string(),
        product_type: ProductType::Pve,
        ..Default::default()
    };
    assert_eq!(verify_serverid(&entry, &info).unwrap(), None);
}
