use bush::{invalidation_scope, InvalidationScope, SceneVersions, Version};

const BASE: SceneVersions = SceneVersions {
    source: Version(1),
    scene: Version(2),
    presentation: Version(3),
};

#[test]
fn reuses_resources_when_versions_match() {
    assert_eq!(invalidation_scope(BASE, BASE), InvalidationScope::Reuse);
}

#[test]
fn presentation_change_does_not_rebuild_scene() {
    let next = SceneVersions {
        presentation: Version(4),
        ..BASE
    };

    assert_eq!(
        invalidation_scope(BASE, next),
        InvalidationScope::UpdatePresentation
    );
}

#[test]
fn scene_change_rebuilds_without_reopening_sources() {
    let next = SceneVersions {
        scene: Version(4),
        ..BASE
    };

    assert_eq!(
        invalidation_scope(BASE, next),
        InvalidationScope::RebuildScene
    );
}

#[test]
fn source_change_wins_over_all_other_changes() {
    let next = SceneVersions {
        source: Version(4),
        scene: Version(5),
        presentation: Version(6),
    };

    assert_eq!(
        invalidation_scope(BASE, next),
        InvalidationScope::ReopenSources
    );
}
