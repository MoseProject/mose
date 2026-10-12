//! QA sweep over the editing commands: random sequences of the real commands, with the project
//! checked after every step. Whatever the user does, the timeline stays well-formed, and undo and
//! redo walk back and forth through exactly the states the edits produced.

use avcore::timeline::Track;

use super::timeline::{
    delete_clips, lift_range, move_clip, ripple_delete_range, roll_edit_clip, slide_clip,
    slip_clip, split_at_playhead, trim_clip, TrimEdge,
};
use super::{redo, undo};
use crate::testing::{harness, Harness};

/// SplitMix64: a small deterministic generator, so a failure reproduces from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }

    fn secs(&mut self, low: f64, high: f64) -> f64 {
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        ((low + (high - low) * unit) * 100.0).round() / 100.0
    }
}

/// A harness holding one fixture clip, with the history of putting it there cleared: the first
/// undo step is then the first edit a test makes.
fn started() -> Harness {
    let h = harness();
    h.add_fixture_clip();
    h.session().undo.clear();
    h
}

fn tracks(h: &Harness) -> Vec<Track> {
    h.session().project.timeline().tracks.clone()
}

/// The clips as `(track, id, start, source in, source out)`: short enough to read in a failure.
fn outline(tracks: &[Track]) -> Vec<(u64, u64, f64, f64, f64)> {
    tracks
        .iter()
        .flat_map(|track| {
            track.clips.iter().map(|c| {
                (
                    track.id,
                    c.id,
                    c.start_secs,
                    c.source_in_secs,
                    c.source_out_secs,
                )
            })
        })
        .collect()
}

fn clip_ids(h: &Harness) -> Vec<u64> {
    tracks(h)
        .iter()
        .flat_map(|track| track.clips.iter().map(|clip| clip.id))
        .collect()
}

fn assert_well_formed(h: &Harness, context: &str) {
    let tracks = tracks(h);
    let mut seen = std::collections::HashSet::new();
    for track in &tracks {
        for clip in &track.clips {
            assert!(
                seen.insert(clip.id),
                "{context}: clip id {} appears twice",
                clip.id
            );
            assert!(
                clip.start_secs.is_finite() && clip.start_secs >= 0.0,
                "{context}: start {} of clip {}",
                clip.start_secs,
                clip.id
            );
            let duration = clip.duration_secs();
            assert!(
                duration.is_finite() && duration > 0.0,
                "{context}: duration {duration} of clip {}",
                clip.id
            );
            assert!(
                clip.source_in_secs >= 0.0,
                "{context}: source_in of clip {}",
                clip.id
            );
        }
    }
}

/// One random edit. Returns nothing: a refused edit is fine, it just must not break anything.
fn random_edit(h: &Harness, rng: &mut Rng) -> usize {
    let which = rng.below(OPS);
    edit(h, rng, which);
    which
}

const OPS: usize = 10;

fn edit(h: &Harness, rng: &mut Rng, which: usize) {
    let ids = clip_ids(h);
    let pick = |rng: &mut Rng| ids[rng.below(ids.len())];
    let end = h.session().project.timeline().duration_secs().max(1.0);
    let handle = h.handle();
    let _ = match which {
        0 => {
            h.set_playhead(rng.secs(0.0, end));
            split_at_playhead(handle, Vec::new())
        }
        1 => delete_clips(handle, vec![pick(rng)], rng.below(2) == 0),
        2 => {
            let id = pick(rng);
            let track_id = tracks(h)
                .iter()
                .find(|t| t.clips.iter().any(|c| c.id == id))
                .map(|t| t.id)
                .unwrap_or(1);
            move_clip(handle, id, track_id, rng.secs(0.0, end))
        }
        3 => trim_clip(handle, pick(rng), TrimEdge::Start, rng.secs(0.0, end)),
        4 => trim_clip(handle, pick(rng), TrimEdge::End, rng.secs(0.0, end * 1.2)),
        5 => slide_clip(handle, pick(rng), rng.secs(0.0, end)),
        6 => roll_edit_clip(handle, pick(rng), rng.secs(0.0, end)),
        7 => slip_clip(handle, pick(rng), rng.secs(-1.0, 1.0)),
        8 => {
            let (a, b) = (rng.secs(0.0, end), rng.secs(0.0, end));
            ripple_delete_range(handle, a.min(b), a.max(b))
        }
        _ => {
            let (a, b) = (rng.secs(0.0, end), rng.secs(0.0, end));
            lift_range(handle, a.min(b), a.max(b))
        }
    };
}

#[test]
fn random_edits_keep_the_timeline_well_formed() {
    for seed in 0..25 {
        let h = harness();
        h.add_fixture_clip();
        let mut rng = Rng(seed);
        for step in 0..40 {
            if clip_ids(&h).is_empty() {
                h.add_fixture_clip();
            }
            random_edit(&h, &mut rng);
            assert_well_formed(&h, &format!("seed {seed} step {step}"));
        }
    }
}

#[test]
fn undo_and_redo_walk_through_exactly_the_states_the_edits_made() {
    for seed in 0..12 {
        let h = started();
        let mut rng = Rng(100 + seed);
        let mut states = vec![tracks(&h)];
        for _ in 0..20 {
            if clip_ids(&h).is_empty() {
                break;
            }
            random_edit(&h, &mut rng);
            let now = tracks(&h);
            // A refused edit must leave no empty undo step: only real changes are recorded.
            if states.last() != Some(&now) {
                states.push(now);
            }
        }
        let context = |what: &str, index: usize| format!("seed {seed}: {what} to state {index}");
        // Back to the start, one step at a time.
        for index in (0..states.len() - 1).rev() {
            undo(h.handle()).unwrap();
            assert_eq!(
                outline(&tracks(&h)),
                outline(&states[index]),
                "{}",
                context("undo", index)
            );
        }
        // Nothing left to undo: one more changes nothing.
        undo(h.handle()).unwrap();
        assert_eq!(tracks(&h), states[0], "seed {seed}: undo past the start");
        // And forward again to the last state.
        for (index, state) in states.iter().enumerate().skip(1) {
            redo(h.handle()).unwrap();
            assert_eq!(&tracks(&h), state, "{}", context("redo", index));
        }
        redo(h.handle()).unwrap();
        assert_eq!(
            &tracks(&h),
            states.last().unwrap(),
            "seed {seed}: redo past the end"
        );
    }
}

#[test]
fn a_new_edit_after_an_undo_drops_the_redo_branch() {
    let h = started();
    let before = tracks(&h);
    h.set_playhead(1.0);
    split_at_playhead(h.handle(), Vec::new()).unwrap();
    undo(h.handle()).unwrap();
    assert_eq!(tracks(&h), before);
    h.set_playhead(2.0);
    split_at_playhead(h.handle(), Vec::new()).unwrap();
    let edited = tracks(&h);
    redo(h.handle()).unwrap();
    assert_eq!(tracks(&h), edited, "redo replayed the abandoned branch");
}

#[test]
fn an_edit_that_changes_nothing_leaves_no_undo_step() {
    let mut offenders = std::collections::BTreeMap::<usize, u64>::new();
    for which in 0..OPS {
        for seed in 0..60 {
            let h = started();
            let mut rng = Rng(1000 + seed);
            // A few edits first so the commands meet clips cut in several pieces.
            for _ in 0..3 {
                random_edit(&h, &mut rng);
                if clip_ids(&h).is_empty() {
                    h.add_fixture_clip();
                }
            }
            h.session().undo.clear();
            let before = tracks(&h);
            edit(&h, &mut rng, which);
            if tracks(&h) == before && h.session().undo.can_undo() {
                offenders.entry(which).or_insert(seed);
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "edits (kind -> first seed) that recorded an undo step without changing the timeline: {offenders:?}"
    );
}
