use std::path::PathBuf;

use bush::{
    Contributor, ContributorContent, ContributorId, MediaSource, RasterOverlay, SceneError,
    SceneSnapshot, SceneVersions, SourceRange, Version,
};

fn media(id: u64, layer: i32, visible: bool, audible: bool) -> Contributor {
    Contributor {
        id: ContributorId(id),
        layer,
        visible,
        audible,
        content: ContributorContent::Media(MediaSource {
            path: PathBuf::from(format!("clip-{id}.mp4")),
            range: SourceRange::new(0.0, 2.0).unwrap(),
            version: Version(1),
        }),
    }
}

#[test]
fn scene_sorts_contributors_once_by_layer_then_identity() {
    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![
            media(3, 2, true, false),
            media(2, 1, true, true),
            media(1, 1, true, true),
        ],
    )
    .unwrap();

    let ids: Vec<_> = scene
        .contributors()
        .iter()
        .map(|contributor| contributor.id)
        .collect();
    assert_eq!(
        ids,
        vec![ContributorId(1), ContributorId(2), ContributorId(3)]
    );
}

#[test]
fn scene_skips_hidden_and_muted_contributors_without_reordering() {
    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![
            media(1, 0, true, false),
            media(2, 1, false, true),
            media(3, 2, true, true),
        ],
    )
    .unwrap();

    let visible: Vec<_> = scene
        .visible_contributors()
        .map(|contributor| contributor.id)
        .collect();
    let audible: Vec<_> = scene
        .audible_contributors()
        .map(|contributor| contributor.id)
        .collect();

    assert_eq!(visible, vec![ContributorId(1), ContributorId(3)]);
    assert_eq!(audible, vec![ContributorId(2), ContributorId(3)]);
}

#[test]
fn scene_rejects_invalid_content_before_runtime_work() {
    let error = SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: false,
            content: ContributorContent::RasterOverlay(RasterOverlay {
                width: 0,
                height: 10,
                version: Version(1),
            }),
        }],
    )
    .unwrap_err();

    assert_eq!(error, SceneError::EmptyRasterOverlay);
}

#[test]
fn scene_rejects_duplicate_contributor_identity() {
    let error = SceneSnapshot::new(
        SceneVersions::default(),
        vec![media(1, 0, true, true), media(1, 1, true, true)],
    )
    .unwrap_err();

    assert_eq!(error, SceneError::DuplicateContributor(ContributorId(1)));
}

#[test]
fn source_range_rejects_nan_negative_and_empty_ranges() {
    assert_eq!(
        SourceRange::new(f64::NAN, 1.0),
        Err(SceneError::NonFiniteSourceRange)
    );
    assert_eq!(
        SourceRange::new(-1.0, 1.0),
        Err(SceneError::InvalidSourceRange {
            in_secs: -1.0,
            out_secs: 1.0,
        })
    );
    assert_eq!(
        SourceRange::new(1.0, 1.0),
        Err(SceneError::InvalidSourceRange {
            in_secs: 1.0,
            out_secs: 1.0,
        })
    );
}
