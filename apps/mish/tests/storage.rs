use mish::storage::{MishStore, default_config};
use serde_json::json;
#[test]
fn simulation_deduplication_signing_and_publication_remain_local_and_private() {
    let state = tempfile::tempdir().unwrap();
    let store = MishStore::new(state.path(), default_config(Some(state.path()))).unwrap();
    let (first, created) = store
        .add_simulated_waypoint(
            44.0,
            -123.0,
            Some("water"),
            None,
            "public",
            None,
            json!({"test":true}),
        )
        .unwrap();
    assert!(created);
    let (second, created) = store
        .add_simulated_waypoint(
            44.0,
            -123.0,
            Some("water"),
            None,
            "public",
            None,
            json!({"test":true}),
        )
        .unwrap();
    assert!(!created);
    assert_eq!(first, second);
    store
        .add_simulated_waypoint(0.0, 0.0, None, None, "private", Some("private"), json!({}))
        .unwrap();
    assert!(
        store
            .create_geopack("leak", vec!["private".into()], "public", Some("leak"))
            .is_err()
    );
    let id = first["id"].as_str().unwrap();
    let (pack, _) = store
        .create_geopack("Venue", vec![id.into()], "public", Some("venue"))
        .unwrap();
    assert!(pack["signer"].as_str().unwrap().starts_with("mish:"));
    let published = store
        .publish_geopack("venue", vec!["mock://check".into()])
        .unwrap();
    assert_eq!(published["relays"][0]["status"], "stored");
    assert!(!published.to_string().contains("private"));
    assert!(
        store
            .add_simulated_waypoint(f64::NAN, 0.0, None, None, "private", None, json!({}))
            .is_err()
    );
    assert!(
        store
            .add_simulated_waypoint(
                0.0,
                0.0,
                None,
                None,
                "private",
                Some("../escape"),
                json!({})
            )
            .is_err()
    );
}
