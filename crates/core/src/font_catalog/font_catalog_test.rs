use super::*;
use sha2::{Digest, Sha256};

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn static_catalog_is_self_consistent() {
    assert_eq!(validate_static_catalog(), Ok(()));
}

#[test]
fn every_text_font_family_variant_has_a_catalog_entry() {
    for family in TextFontFamily::ALL {
        let id = family.family_id();
        let entry = find_family(id).unwrap_or_else(|| panic!("no catalog entry for {id:?}"));
        assert_eq!(entry.family_id, id);
    }
}

#[test]
fn family_id_round_trips_through_from_family_id() {
    for family in TextFontFamily::ALL {
        assert_eq!(
            TextFontFamily::from_family_id(family.family_id()),
            Some(family)
        );
    }
}

#[test]
fn from_family_id_rejects_unknown_slugs() {
    assert_eq!(TextFontFamily::from_family_id("comic-sans"), None);
    assert_eq!(TextFontFamily::from_family_id(""), None);
}

#[test]
fn every_fallback_chain_terminates_at_lato_within_one_hop() {
    // A stronger property than validate_static_catalog()'s own "fallback id exists" check:
    // the doc's fallback policy is "Lato Regular for GF Latin Core characters missing because
    // of a corrupt/invalid selection" specifically, not an arbitrary chain.
    for entry in CATALOG {
        match entry.fallback_family_id {
            None => assert_eq!(entry.family_id, "lato"),
            Some(fallback) => assert_eq!(fallback, "lato"),
        }
    }
}

// Bundled-byte verification: include_bytes! needs a literal path per call site, so this can't
// loop over CATALOG's own source_path strings — each family's faces are checked individually
// against the exact bytes compiled into the binary, catching a manifest hash/size that has
// drifted from the actual vendored file (the doc's "Verify SHA-256" requirement) for real, not
// just a manifest self-consistency check.

fn assert_face_matches(family_id: &str, source_path: &str, bytes: &[u8]) {
    let entry = find_family(family_id).expect("family missing from catalog");
    let face = entry
        .faces
        .iter()
        .find(|f| f.source_path == source_path)
        .unwrap_or_else(|| panic!("no face {source_path:?} on family {family_id:?}"));
    assert_eq!(
        face.size_bytes,
        bytes.len() as u64,
        "size_bytes drifted for {source_path}"
    );
    assert_eq!(
        face.sha256,
        hex_sha256(bytes),
        "sha256 drifted for {source_path}"
    );
}

#[test]
fn locked_face_bytes_has_exactly_one_entry_per_catalog_face() {
    let expected: usize = CATALOG.iter().map(|entry| entry.faces.len()).sum();
    assert_eq!(locked_face_bytes().len(), expected);
    for entry in CATALOG {
        for face in entry.faces {
            let bytes = face_bytes(entry.family_id, face.weight).unwrap_or_else(|| {
                panic!(
                    "locked_face_bytes() has no entry for ({}, {})",
                    entry.family_id, face.weight
                )
            });
            assert_eq!(bytes.len() as u64, face.size_bytes);
            assert_eq!(hex_sha256(bytes), face.sha256);
        }
    }
}

#[test]
fn face_bytes_returns_none_for_an_unknown_family_or_weight() {
    assert!(face_bytes("comic-sans", 400).is_none());
    assert!(face_bytes("lato", 900).is_none());
}

#[test]
fn bundled_lato_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "lato",
        "lato/Lato-Regular.ttf",
        include_bytes!("../../assets/fonts/lato/Lato-Regular.ttf"),
    );
    assert_face_matches(
        "lato",
        "lato/Lato-Bold.ttf",
        include_bytes!("../../assets/fonts/lato/Lato-Bold.ttf"),
    );
}

#[test]
fn bundled_bebas_neue_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "bebas-neue",
        "bebas-neue/BebasNeue-Regular.ttf",
        include_bytes!("../../assets/fonts/bebas-neue/BebasNeue-Regular.ttf"),
    );
}

#[test]
fn bundled_playfair_display_sc_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "playfair-display-sc",
        "playfair-display-sc/PlayfairDisplaySC-Regular.ttf",
        include_bytes!("../../assets/fonts/playfair-display-sc/PlayfairDisplaySC-Regular.ttf"),
    );
    assert_face_matches(
        "playfair-display-sc",
        "playfair-display-sc/PlayfairDisplaySC-Bold.ttf",
        include_bytes!("../../assets/fonts/playfair-display-sc/PlayfairDisplaySC-Bold.ttf"),
    );
}

#[test]
fn bundled_patrick_hand_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "patrick-hand",
        "patrick-hand/PatrickHand-Regular.ttf",
        include_bytes!("../../assets/fonts/patrick-hand/PatrickHand-Regular.ttf"),
    );
}

#[test]
fn bundled_anonymous_pro_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "anonymous-pro",
        "anonymous-pro/AnonymousPro-Regular.ttf",
        include_bytes!("../../assets/fonts/anonymous-pro/AnonymousPro-Regular.ttf"),
    );
    assert_face_matches(
        "anonymous-pro",
        "anonymous-pro/AnonymousPro-Bold.ttf",
        include_bytes!("../../assets/fonts/anonymous-pro/AnonymousPro-Bold.ttf"),
    );
}

#[test]
fn bundled_archivo_black_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "archivo-black",
        "archivo-black/ArchivoBlack-Regular.ttf",
        include_bytes!("../../assets/fonts/archivo-black/ArchivoBlack-Regular.ttf"),
    );
}

#[test]
fn bundled_inter_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "inter",
        "inter/Inter[opsz,wght].ttf",
        include_bytes!("../../assets/fonts/inter/Inter[opsz,wght].ttf"),
    );
}

#[test]
fn bundled_montserrat_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "montserrat",
        "montserrat/Montserrat[wght].ttf",
        include_bytes!("../../assets/fonts/montserrat/Montserrat[wght].ttf"),
    );
}

#[test]
fn bundled_roboto_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "roboto",
        "roboto/Roboto[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/roboto/Roboto[wdth,wght].ttf"),
    );
}

#[test]
fn bundled_open_sans_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "open-sans",
        "open-sans/OpenSans[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/open-sans/OpenSans[wdth,wght].ttf"),
    );
}

#[test]
fn bundled_poppins_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "poppins",
        "poppins/Poppins-Regular.ttf",
        include_bytes!("../../assets/fonts/poppins/Poppins-Regular.ttf"),
    );
    assert_face_matches(
        "poppins",
        "poppins/Poppins-Bold.ttf",
        include_bytes!("../../assets/fonts/poppins/Poppins-Bold.ttf"),
    );
}

#[test]
fn bundled_nunito_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "nunito",
        "nunito/Nunito[wght].ttf",
        include_bytes!("../../assets/fonts/nunito/Nunito[wght].ttf"),
    );
}

#[test]
fn bundled_source_sans_3_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "source-sans-3",
        "source-sans-3/SourceSans3[wght].ttf",
        include_bytes!("../../assets/fonts/source-sans-3/SourceSans3[wght].ttf"),
    );
}

#[test]
fn bundled_barlow_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "barlow",
        "barlow/Barlow-Regular.ttf",
        include_bytes!("../../assets/fonts/barlow/Barlow-Regular.ttf"),
    );
    assert_face_matches(
        "barlow",
        "barlow/Barlow-Bold.ttf",
        include_bytes!("../../assets/fonts/barlow/Barlow-Bold.ttf"),
    );
}

#[test]
fn bundled_fredoka_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "fredoka",
        "fredoka/Fredoka[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/fredoka/Fredoka[wdth,wght].ttf"),
    );
}

#[test]
fn bundled_oswald_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "oswald",
        "oswald/Oswald[wght].ttf",
        include_bytes!("../../assets/fonts/oswald/Oswald[wght].ttf"),
    );
}

#[test]
fn bundled_anton_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "anton",
        "anton/Anton-Regular.ttf",
        include_bytes!("../../assets/fonts/anton/Anton-Regular.ttf"),
    );
}

#[test]
fn bundled_barlow_condensed_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "barlow-condensed",
        "barlow-condensed/BarlowCondensed-Regular.ttf",
        include_bytes!("../../assets/fonts/barlow-condensed/BarlowCondensed-Regular.ttf"),
    );
    assert_face_matches(
        "barlow-condensed",
        "barlow-condensed/BarlowCondensed-Bold.ttf",
        include_bytes!("../../assets/fonts/barlow-condensed/BarlowCondensed-Bold.ttf"),
    );
}

#[test]
fn bundled_league_spartan_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "league-spartan",
        "league-spartan/LeagueSpartan[wght].ttf",
        include_bytes!("../../assets/fonts/league-spartan/LeagueSpartan[wght].ttf"),
    );
}

#[test]
fn bundled_teko_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "teko",
        "teko/Teko[wght].ttf",
        include_bytes!("../../assets/fonts/teko/Teko[wght].ttf"),
    );
}

#[test]
fn bundled_black_ops_one_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "black-ops-one",
        "black-ops-one/BlackOpsOne-Regular.ttf",
        include_bytes!("../../assets/fonts/black-ops-one/BlackOpsOne-Regular.ttf"),
    );
}

#[test]
fn bundled_russo_one_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "russo-one",
        "russo-one/RussoOne-Regular.ttf",
        include_bytes!("../../assets/fonts/russo-one/RussoOne-Regular.ttf"),
    );
}

#[test]
fn bundled_bangers_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "bangers",
        "bangers/Bangers-Regular.ttf",
        include_bytes!("../../assets/fonts/bangers/Bangers-Regular.ttf"),
    );
}

#[test]
fn bundled_merriweather_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "merriweather",
        "merriweather/Merriweather[opsz,wdth,wght].ttf",
        include_bytes!("../../assets/fonts/merriweather/Merriweather[opsz,wdth,wght].ttf"),
    );
}

#[test]
fn bundled_libre_baskerville_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "libre-baskerville",
        "libre-baskerville/LibreBaskerville[wght].ttf",
        include_bytes!("../../assets/fonts/libre-baskerville/LibreBaskerville[wght].ttf"),
    );
}

#[test]
fn bundled_lora_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "lora",
        "lora/Lora[wght].ttf",
        include_bytes!("../../assets/fonts/lora/Lora[wght].ttf"),
    );
}

#[test]
fn bundled_cinzel_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "cinzel",
        "cinzel/Cinzel[wght].ttf",
        include_bytes!("../../assets/fonts/cinzel/Cinzel[wght].ttf"),
    );
}

#[test]
fn bundled_bitter_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "bitter",
        "bitter/Bitter[wght].ttf",
        include_bytes!("../../assets/fonts/bitter/Bitter[wght].ttf"),
    );
}

#[test]
fn bundled_caveat_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "caveat",
        "caveat/Caveat[wght].ttf",
        include_bytes!("../../assets/fonts/caveat/Caveat[wght].ttf"),
    );
}

#[test]
fn bundled_pacifico_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "pacifico",
        "pacifico/Pacifico-Regular.ttf",
        include_bytes!("../../assets/fonts/pacifico/Pacifico-Regular.ttf"),
    );
}

#[test]
fn bundled_dancing_script_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "dancing-script",
        "dancing-script/DancingScript[wght].ttf",
        include_bytes!("../../assets/fonts/dancing-script/DancingScript[wght].ttf"),
    );
}

#[test]
fn bundled_comic_neue_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "comic-neue",
        "comic-neue/ComicNeue-Regular.ttf",
        include_bytes!("../../assets/fonts/comic-neue/ComicNeue-Regular.ttf"),
    );
    assert_face_matches(
        "comic-neue",
        "comic-neue/ComicNeue-Bold.ttf",
        include_bytes!("../../assets/fonts/comic-neue/ComicNeue-Bold.ttf"),
    );
}

#[test]
fn bundled_gloria_hallelujah_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "gloria-hallelujah",
        "gloria-hallelujah/GloriaHallelujah.ttf",
        include_bytes!("../../assets/fonts/gloria-hallelujah/GloriaHallelujah.ttf"),
    );
}

#[test]
fn bundled_jetbrains_mono_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "jetbrains-mono",
        "jetbrains-mono/JetBrainsMono[wght].ttf",
        include_bytes!("../../assets/fonts/jetbrains-mono/JetBrainsMono[wght].ttf"),
    );
}

#[test]
fn bundled_roboto_mono_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "roboto-mono",
        "roboto-mono/RobotoMono[wght].ttf",
        include_bytes!("../../assets/fonts/roboto-mono/RobotoMono[wght].ttf"),
    );
}

#[test]
fn bundled_space_mono_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "space-mono",
        "space-mono/SpaceMono-Regular.ttf",
        include_bytes!("../../assets/fonts/space-mono/SpaceMono-Regular.ttf"),
    );
    assert_face_matches(
        "space-mono",
        "space-mono/SpaceMono-Bold.ttf",
        include_bytes!("../../assets/fonts/space-mono/SpaceMono-Bold.ttf"),
    );
}

#[test]
fn bundled_noto_sans_arabic_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "noto-sans-arabic",
        "noto-sans-arabic/NotoSansArabic[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/noto-sans-arabic/NotoSansArabic[wdth,wght].ttf"),
    );
}

#[test]
fn bundled_noto_naskh_arabic_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "noto-naskh-arabic",
        "noto-naskh-arabic/NotoNaskhArabic[wght].ttf",
        include_bytes!("../../assets/fonts/noto-naskh-arabic/NotoNaskhArabic[wght].ttf"),
    );
}

#[test]
fn bundled_noto_sans_hebrew_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "noto-sans-hebrew",
        "noto-sans-hebrew/NotoSansHebrew[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/noto-sans-hebrew/NotoSansHebrew[wdth,wght].ttf"),
    );
}

#[test]
fn bundled_noto_sans_devanagari_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "noto-sans-devanagari",
        "noto-sans-devanagari/NotoSansDevanagari[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/noto-sans-devanagari/NotoSansDevanagari[wdth,wght].ttf"),
    );
}

#[test]
fn bundled_noto_sans_bengali_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "noto-sans-bengali",
        "noto-sans-bengali/NotoSansBengali[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/noto-sans-bengali/NotoSansBengali[wdth,wght].ttf"),
    );
}

#[test]
fn bundled_noto_sans_tamil_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "noto-sans-tamil",
        "noto-sans-tamil/NotoSansTamil[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/noto-sans-tamil/NotoSansTamil[wdth,wght].ttf"),
    );
}

#[test]
fn bundled_noto_sans_thai_bytes_match_the_locked_manifest() {
    assert_face_matches(
        "noto-sans-thai",
        "noto-sans-thai/NotoSansThai[wdth,wght].ttf",
        include_bytes!("../../assets/fonts/noto-sans-thai/NotoSansThai[wdth,wght].ttf"),
    );
}

#[test]
fn catalog_has_exactly_43_families_and_51_faces() {
    // The doc's own release gate: "exactly 43 visible families and 51 approved source binaries
    // unless this document and the lock manifest are reviewed together."
    assert_eq!(CATALOG.len(), 43);
    let total_faces: usize = CATALOG.iter().map(|entry| entry.faces.len()).sum();
    assert_eq!(total_faces, 51);
}

#[test]
fn license_and_source_paths_exist_on_disk_and_never_traverse() {
    // Doc's "static destination allowlist" concern: every path is a plain relative filename
    // under the fonts asset root, never `..` or absolute — and it actually resolves to a real
    // file, not just a plausible-looking string in the manifest.
    let assets_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    for entry in CATALOG {
        assert!(
            !entry.license_path.contains("..")
                && !std::path::Path::new(entry.license_path).is_absolute(),
            "license_path must not traverse: {}",
            entry.license_path
        );
        assert!(
            assets_root.join(entry.license_path).is_file(),
            "license_path does not exist on disk: {}",
            entry.license_path
        );
        for face in entry.faces {
            assert!(
                !face.source_path.contains("..")
                    && !std::path::Path::new(face.source_path).is_absolute(),
                "source_path must not traverse: {}",
                face.source_path
            );
            assert!(
                assets_root.join(face.source_path).is_file(),
                "source_path does not exist on disk: {}",
                face.source_path
            );
        }
    }
}

const GOOD_HASH: &str = "d636e4683231f931eda222d588e944d082bfd3bdba02f928bee461c0f185b251";

fn broken_entry() -> FontFamilyEntry {
    FontFamilyEntry {
        family_id: "good-id",
        display_name: "Good",
        category: FontCategory::Sans,
        tags: &[],
        license_path: "good/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "good/Good-Regular.ttf",
            sha256: GOOD_HASH,
            size_bytes: 1,
        }],
        default_weight: 400,
        fallback_family_id: None,
        glyphset_guarantees: &[],
    }
}

fn errors_for(entries: &[FontFamilyEntry]) -> String {
    validate_catalog(entries).unwrap_err().join("\n")
}

#[test]
fn a_consistent_catalog_validates() {
    assert!(validate_catalog(&[broken_entry()]).is_ok());
}

#[test]
fn every_catalog_inconsistency_is_reported() {
    let bad_slug = FontFamilyEntry {
        family_id: "Bad Slug",
        ..broken_entry()
    };
    assert!(errors_for(&[bad_slug]).contains("lowercase ASCII slug"));
    let empty_slug = FontFamilyEntry {
        family_id: "",
        ..broken_entry()
    };
    assert!(errors_for(&[empty_slug]).contains("lowercase ASCII slug"));

    // The same id twice, which also reuses the same face file.
    let duplicates = errors_for(&[broken_entry(), broken_entry()]);
    assert!(duplicates.contains("duplicate family_id"));
    assert!(duplicates.contains("duplicate source_path"));

    let no_faces = FontFamilyEntry {
        faces: &[],
        ..broken_entry()
    };
    let message = errors_for(&[no_faces]);
    assert!(message.contains("declares no faces"));
    assert!(message.contains("default_weight"));

    let wrong_default = FontFamilyEntry {
        default_weight: 900,
        ..broken_entry()
    };
    assert!(errors_for(&[wrong_default]).contains("default_weight 900"));

    let no_license = FontFamilyEntry {
        license_id: "",
        ..broken_entry()
    };
    assert!(errors_for(&[no_license]).contains("missing license metadata"));

    let own_fallback = FontFamilyEntry {
        fallback_family_id: Some("good-id"),
        ..broken_entry()
    };
    assert!(errors_for(&[own_fallback]).contains("its own fallback"));
    let unknown_fallback = FontFamilyEntry {
        fallback_family_id: Some("nowhere"),
        ..broken_entry()
    };
    assert!(errors_for(&[unknown_fallback]).contains("unknown fallback_family_id"));

    const SHORT_HASH_FACE: &[FontFace] = &[FontFace {
        weight: 400,
        source_path: "good/Short.ttf",
        sha256: "abc",
        size_bytes: 1,
    }];
    let bad_hash = FontFamilyEntry {
        faces: SHORT_HASH_FACE,
        ..broken_entry()
    };
    assert!(errors_for(&[bad_hash]).contains("malformed sha256"));
}
