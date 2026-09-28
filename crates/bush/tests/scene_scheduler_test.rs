use std::path::PathBuf;

use bush::{
    Contributor, ContributorContent, ContributorId, Generation, InvalidationScope, MediaSource,
    SceneScheduler, SceneSnapshot, SceneVersions, SourceRange, Version,
};

fn scene(versions: SceneVersions) -> SceneSnapshot {
    SceneSnapshot::new(
        versions,
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: true,
            content: ContributorContent::Media(MediaSource {
                path: PathBuf::from("clip.mp4"),
                range: SourceRange::new(0.0, 1.0).unwrap(),
                version: versions.source,
            }),
        }],
    )
    .unwrap()
}

const FIRST: SceneVersions = SceneVersions {
    source: Version(1),
    scene: Version(1),
    presentation: Version(1),
};

#[test]
fn initial_scene_reopens_sources() {
    let mut scheduler = SceneScheduler::new();
    assert_eq!(
        scheduler.submit(scene(FIRST)).unwrap().generation,
        Generation(1)
    );

    let scheduled = scheduler.take_next().unwrap();
    assert_eq!(scheduled.invalidation, InvalidationScope::ReopenSources);
    assert!(scheduler.is_idle());
}

#[test]
fn later_submit_replaces_unstarted_scene() {
    let mut scheduler = SceneScheduler::new();
    let first = scheduler.submit(scene(FIRST)).unwrap();
    let latest = scheduler
        .submit(scene(SceneVersions {
            presentation: Version(2),
            ..FIRST
        }))
        .unwrap();

    assert_eq!(first.generation, Generation(1));
    assert_eq!(latest.generation, Generation(2));
    assert_eq!(latest.superseded, Some(Generation(1)));
    assert_eq!(scheduler.take_next().unwrap().generation, Generation(2));
}

#[test]
fn invalidation_uses_last_applied_scene_not_replaced_pending_scene() {
    let mut scheduler = SceneScheduler::new();
    scheduler.submit(scene(FIRST)).unwrap();
    let initial = scheduler.take_next().unwrap();
    scheduler.mark_applied(&initial.scene);

    scheduler
        .submit(scene(SceneVersions {
            presentation: Version(2),
            ..FIRST
        }))
        .unwrap();
    scheduler
        .submit(scene(SceneVersions {
            scene: Version(2),
            presentation: Version(3),
            ..FIRST
        }))
        .unwrap();

    let scheduled = scheduler.take_next().unwrap();
    assert_eq!(scheduled.invalidation, InvalidationScope::RebuildScene);
}
