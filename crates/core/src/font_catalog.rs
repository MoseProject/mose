// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! FONT-01A/B: the locked built-in font catalog manifest
//! ([`spec/architecture/built-in-font-catalog.md`](../../../../spec/architecture/built-in-font-catalog.md)).
//!
//! [`TextFontFamily`] now has one variant per [`CATALOG`] entry — all 43 families, matching one
//! to one via [`TextFontFamily::family_id`]/[`TextFontFamily::from_family_id`]; the manifest
//! fields mirror the doc's required typed fields (`family_id`, `source_path`, `sha256`, license,
//! weights, ...). **FONT-01B is now fully vendored and selectable**: [`CATALOG`] holds all 43
//! families (51 binaries) the doc specifies — Sans (10), Display/condensed/gaming (10), Serif
//! (6), Handwritten (6), Monospace (4), and the International shaping/fallback set (7 `Noto`
//! faces, [`FontCategory::International`]) — every one of them reachable from the Editor's
//! text-clip font picker, not just the original six. `loads_exactly_the_locked_catalog_faces_
//! and_nothing_else` confirms all 51 binaries load through the real production `fontdb` path;
//! `every_bundled_family_shapes_ordinary_latin_text_without_missing_glyphs` now iterates
//! [`TextFontFamily::ALL`]'s full 43 entries (it only needed the enum to grow, no test code
//! changed) and confirmed — genuinely, not assumed — that every one of them, *including* the 7
//! `Noto` international faces, shapes ordinary Portuguese/Latin text with zero `.notdef` hits.
//! **Still deferred**: the doc's forwards-compatible persisted-identity swap (`.ocproj` storing
//! a `family_id` slug instead of the plain enum variant name, so an unrecognized *future* id
//! keeps its exact string through a resave instead of normalizing to `Lato`) — a separate,
//! larger structural change; adding 37 plain enum variants already gives the *practical*
//! "selectable and persists correctly" outcome without it (see [`TextFontFamily`]'s own doc
//! comment). FONT-01C's categorized/searchable selector and FONT-01D's variable-axis selection
//! UI are also still open — today's picker is the same flat list it always was, just longer.
//!
//! Static data only. Acquisition/vendoring is a maintainer-time, offline operation per the doc's
//! "Acquisition and update workflow" section — no network code belongs in this module, and none
//! exists here. Every binary under `crates/core/assets/fonts/` is vendored from the pinned
//! [`GOOGLE_FONTS_REVISION`], downloaded and SHA-256/size-verified against the real upstream
//! files at that exact commit, never a later or different revision — total bundled size 18 MiB,
//! inside the doc's 20 MiB release gate.

use crate::timeline::TextFontFamily;

/// Revision of the pinned `google/fonts` upstream repository this catalog's binaries were
/// vendored from, per the doc's "Measured package budget" section.
pub const GOOGLE_FONTS_REVISION: &str = "ade3d1533e06b2b1462ffcde8e08b129627ca360";

/// Schema version of this manifest's own shape. Bump only when a field's meaning changes, not
/// when a family is added.
pub const CATALOG_VERSION: u32 = 1;

/// Curated grouping used by the font selector's category sections (doc: "sans, display, serif,
/// handwritten, monospace").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontCategory {
    Sans,
    Display,
    Serif,
    Handwritten,
    Monospace,
    /// The doc's "International shaping and fallback" set (section: "These faces are visible
    /// under an International category but also form TEXT-01's deterministic fallback
    /// chains"). Selectable from the Editor's font picker like any other family; using them as
    /// TEXT-01's own automatic script-fallback chain (rather than a manual pick) is still gated
    /// on that feature's own shaping tests.
    International,
}

/// Whether a family's upstream binaries are static per-weight files or one variable-axis file.
/// FONT-01A's original six families are all `Static`; FONT-01B's own variable families
/// (`Inter[opsz,wght].ttf` and friends) use `Variable`, slotting into the same shape without a
/// breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSourceKind {
    Static,
    Variable,
}

/// One locked, on-disk font binary within a family — a static weight/style file today; a single
/// variable-axis file (one entry covering every weight) once FONT-01B lands those.
#[derive(Debug, Clone, Copy)]
pub struct FontFace {
    /// Numeric weight this face renders as (400 = Regular, 700 = Bold, ...).
    pub weight: u16,
    /// Path to the vendored binary, relative to `crates/core/assets/fonts/`.
    pub source_path: &'static str,
    /// Lowercase hex SHA-256 of the exact bytes at `source_path`.
    pub sha256: &'static str,
    pub size_bytes: u64,
}

/// One catalog family — the manifest unit the doc describes, keyed by a stable `family_id` slug
/// that is never localized and never changes meaning once shipped.
#[derive(Debug, Clone, Copy)]
pub struct FontFamilyEntry {
    /// Stable ASCII slug, e.g. `"lato"`. This is the catalog's persisted identity target
    /// (FONT-01A's remaining `.ocproj` migration, not yet wired) — never a translated
    /// `display_name`, never an enum ordinal.
    pub family_id: &'static str,
    pub display_name: &'static str,
    pub category: FontCategory,
    pub tags: &'static [&'static str],
    /// Path to the vendored `OFL.txt`, relative to `crates/core/assets/fonts/`.
    pub license_path: &'static str,
    pub license_id: &'static str,
    pub source_kind: FontSourceKind,
    /// One or more locked binaries. Every weight in a static family's own file set; exactly one
    /// entry for a variable family (FONT-01B).
    pub faces: &'static [FontFace],
    /// Weight selected when a project doesn't otherwise specify one.
    pub default_weight: u16,
    /// `family_id` of the deterministic fallback used when this family's own bytes fail to
    /// parse (doc: "Lato Regular for GF Latin Core characters missing because of a
    /// corrupt/invalid selection"). `None` only for the fallback family itself.
    pub fallback_family_id: Option<&'static str>,
    /// Coarse glyph-coverage claims this entry backs. `"gf-latin-core"` covers the acceptance
    /// corpus in the doc's "Shaping and script support" section (Portuguese/Spanish/English) —
    /// verified for every family via `text_layout`'s own shaping test (see this module's doc
    /// comment). `"international-fallback"` additionally marks the 7 `Noto` entries vendored
    /// specifically for their own non-Latin script.
    pub glyphset_guarantees: &'static [&'static str],
}

const GF_LATIN_CORE: &[&str] = &["gf-latin-core"];

/// The locked catalog: all 43 bundled families, one entry each — FONT-01A's original six (order
/// matching [`TextFontFamily::ALL`]) followed by FONT-01B's remaining 37 (the doc's Sans,
/// Display/condensed/gaming, Serif, Handwritten, Monospace, and International tables, in that
/// order). Matches the asset README's own table.
pub const CATALOG: &[FontFamilyEntry] = &[
    FontFamilyEntry {
        family_id: "lato",
        display_name: "Lato",
        category: FontCategory::Sans,
        tags: &["caption", "general"],
        license_path: "lato/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[
            FontFace {
                weight: 400,
                source_path: "lato/Lato-Regular.ttf",
                sha256: "d636e4683231f931eda222d588e944d082bfd3bdba02f928bee461c0f185b251",
                size_bytes: 656_568,
            },
            FontFace {
                weight: 700,
                source_path: "lato/Lato-Bold.ttf",
                sha256: "8a0aace75d33794eece4b28187bfc1df0bbd2888b5d8a56e01788c8d65d16be1",
                size_bytes: 656_544,
            },
        ],
        default_weight: 400,
        fallback_family_id: None,
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "bebas-neue",
        display_name: "Bebas Neue",
        category: FontCategory::Display,
        tags: &["condensed", "title"],
        license_path: "bebas-neue/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "bebas-neue/BebasNeue-Regular.ttf",
            sha256: "08e4623805102d819f58601e46e345648846075e363b2ceb23313c2d1c83ec73",
            size_bytes: 61_400,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "playfair-display-sc",
        display_name: "Playfair Display SC",
        category: FontCategory::Serif,
        tags: &["serif", "high-contrast"],
        license_path: "playfair-display-sc/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[
            FontFace {
                weight: 400,
                source_path: "playfair-display-sc/PlayfairDisplaySC-Regular.ttf",
                sha256: "55df09f0a49905a0f5e478597b5b3fc78acf0512ceb568504f3cdc4659662e47",
                size_bytes: 201_772,
            },
            FontFace {
                weight: 700,
                source_path: "playfair-display-sc/PlayfairDisplaySC-Bold.ttf",
                sha256: "b58a968cfc9b4e96e6857a206e1b042dc3fdbe5dc31cd75825d864f80ac7e915",
                size_bytes: 204_568,
            },
        ],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "patrick-hand",
        display_name: "Patrick Hand",
        category: FontCategory::Handwritten,
        tags: &["handwritten", "casual"],
        license_path: "patrick-hand/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "patrick-hand/PatrickHand-Regular.ttf",
            sha256: "0f173b3e6cb6d1af25babf7f0057c5ac4ee11f9992b0469bb817e967ef4ad0fc",
            size_bytes: 214_772,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "anonymous-pro",
        display_name: "Anonymous Pro",
        category: FontCategory::Monospace,
        tags: &["monospace", "code"],
        license_path: "anonymous-pro/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[
            FontFace {
                weight: 400,
                source_path: "anonymous-pro/AnonymousPro-Regular.ttf",
                sha256: "46d8b9a5f4b38fc9d30f3cdd676d4c6f78a9bef949bb1a8304216cc731eb87f8",
                size_bytes: 158_100,
            },
            FontFace {
                weight: 700,
                source_path: "anonymous-pro/AnonymousPro-Bold.ttf",
                sha256: "52895657e7d48860089ebfac63a27244f37a2218ec9c5019a0b37fe885c0fc0c",
                size_bytes: 153_616,
            },
        ],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "archivo-black",
        display_name: "Archivo Black",
        category: FontCategory::Display,
        tags: &["bold", "caption"],
        license_path: "archivo-black/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "archivo-black/ArchivoBlack-Regular.ttf",
            sha256: "dd9a89a019b4849f66ab75455fe7bdf931311042cbb0f0f97acc061539703180",
            size_bytes: 90_988,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    // FONT-01B slice 1 (`architecture/built-in-font-catalog.md`'s Sans table, minus Lato which
    // was already bundled) — vendored from the same pinned `GOOGLE_FONTS_REVISION` the doc's
    // README table names, sha256/size verified against the exact bytes committed under
    // `crates/core/assets/fonts/`. The remaining categories (Display/Condensed, Serif,
    // Handwritten, Monospace, international fallback) are still open — see `ROADMAP.md`'s own
    // FONT-01 entry.
    FontFamilyEntry {
        family_id: "inter",
        display_name: "Inter",
        category: FontCategory::Sans,
        tags: &["ui", "editorial"],
        license_path: "inter/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "inter/Inter[opsz,wght].ttf",
            sha256: "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031",
            size_bytes: 876_576,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "montserrat",
        display_name: "Montserrat",
        category: FontCategory::Sans,
        tags: &["geometric", "title"],
        license_path: "montserrat/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "montserrat/Montserrat[wght].ttf",
            sha256: "0f7b311b2f3279e4eef9b2f968bcdbab6e28f4daeb1f049f4f278a902bcd82f7",
            size_bytes: 744_936,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "roboto",
        display_name: "Roboto",
        category: FontCategory::Sans,
        tags: &["dense", "overlay"],
        license_path: "roboto/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "roboto/Roboto[wdth,wght].ttf",
            sha256: "d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134",
            size_bytes: 488_584,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "open-sans",
        display_name: "Open Sans",
        category: FontCategory::Sans,
        tags: &["readable", "caption"],
        license_path: "open-sans/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "open-sans/OpenSans[wdth,wght].ttf",
            sha256: "36643644f318a812aab2d2ed3bb98f8cf0872527f835fe9398d95fe6b9adb878",
            size_bytes: 532_636,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "poppins",
        display_name: "Poppins",
        category: FontCategory::Sans,
        tags: &["geometric", "social"],
        license_path: "poppins/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[
            FontFace {
                weight: 400,
                source_path: "poppins/Poppins-Regular.ttf",
                sha256: "7e65201e9b79159e2300267cc885e16c8dcef2424cdfa09a29bfb0980a94a7ba",
                size_bytes: 160_316,
            },
            FontFace {
                weight: 700,
                source_path: "poppins/Poppins-Bold.ttf",
                sha256: "983676516167748b74de6f4771fb384c664fd913acb8b471122ecacf5da5ea6c",
                size_bytes: 155_996,
            },
        ],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "nunito",
        display_name: "Nunito",
        category: FontCategory::Sans,
        tags: &["rounded", "friendly"],
        license_path: "nunito/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "nunito/Nunito[wght].ttf",
            sha256: "bb55a5ca5c2042335b3991af27c4d0705d0ef41cac6164ac737fd8f2a1e85207",
            size_bytes: 276_932,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "source-sans-3",
        display_name: "Source Sans 3",
        category: FontCategory::Sans,
        tags: &["editorial", "tutorial"],
        license_path: "source-sans-3/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "source-sans-3/SourceSans3[wght].ttf",
            sha256: "042fe2cc0b933e328410d7acbd0aa6a1873dca5aef81875f4bc214b08825c7b9",
            size_bytes: 646_340,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "barlow",
        display_name: "Barlow",
        category: FontCategory::Sans,
        tags: &["compact", "overlay"],
        license_path: "barlow/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[
            FontFace {
                weight: 400,
                source_path: "barlow/Barlow-Regular.ttf",
                sha256: "95aa02c7c43096e0dd44d787ba6216864a67157e402adab59b35572e0c1577ea",
                size_bytes: 104_068,
            },
            FontFace {
                weight: 700,
                source_path: "barlow/Barlow-Bold.ttf",
                sha256: "84e6a4d61e7c3e21f3c50ea6a4f7e5303a3467864c038be6ea3759bab8d547f9",
                size_bytes: 108_220,
            },
        ],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "fredoka",
        display_name: "Fredoka",
        category: FontCategory::Sans,
        tags: &["rounded", "playful"],
        license_path: "fredoka/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "fredoka/Fredoka[wdth,wght].ttf",
            sha256: "2ba02e68b152868aef9ba28e24b3648c7d457fe6f25c761f2c2c53fb61a73fc8",
            size_bytes: 159_184,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    // FONT-01B slice 2: Display/condensed/gaming (8 new, Bebas Neue/Archivo Black already
    // bundled), Serif (5 new), Handwritten (5 new), Monospace (3 new), and International (7
    // new) -- the doc's remaining four categories plus the shaping-fallback set, completing all
    // 43 families/51 binaries. Same pinned GOOGLE_FONTS_REVISION, same sha256/size verification.
    FontFamilyEntry {
        family_id: "oswald",
        display_name: "Oswald",
        category: FontCategory::Display,
        tags: &["condensed", "subtitle"],
        license_path: "oswald/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "oswald/Oswald[wght].ttf",
            sha256: "5b38c246e255a12f5712d640d56bcced0472466fc68983d2d0410ec0457c2817",
            size_bytes: 172_088,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "anton",
        display_name: "Anton",
        category: FontCategory::Display,
        tags: &["high-impact", "title"],
        license_path: "anton/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "anton/Anton-Regular.ttf",
            sha256: "a4ba3a92350ebb031da0cb47630ac49eb265082ca1bc0450442f4a83ab947cab",
            size_bytes: 170_812,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "barlow-condensed",
        display_name: "Barlow Condensed",
        category: FontCategory::Display,
        tags: &["condensed", "hud"],
        license_path: "barlow-condensed/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[
            FontFace {
                weight: 400,
                source_path: "barlow-condensed/BarlowCondensed-Regular.ttf",
                sha256: "583cec5da3b84bc4dc7c9c72e2a565c94d34e431518b19d7e250b7830ad5f996",
                size_bytes: 102_480,
            },
            FontFace {
                weight: 700,
                source_path: "barlow-condensed/BarlowCondensed-Bold.ttf",
                sha256: "e476562ec9c1e16cf16475895b511f08c804f438cc9a9f80a44ea50a0eeb5b65",
                size_bytes: 109_912,
            },
        ],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "league-spartan",
        display_name: "League Spartan",
        category: FontCategory::Display,
        tags: &["geometric", "title"],
        license_path: "league-spartan/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "league-spartan/LeagueSpartan[wght].ttf",
            sha256: "2dbb6290b39ab7c48a40b18f74ca59ef48a69a015c3ea0542703f0c6ce51d617",
            size_bytes: 95_116,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "teko",
        display_name: "Teko",
        category: FontCategory::Display,
        tags: &["narrow", "esports"],
        license_path: "teko/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "teko/Teko[wght].ttf",
            sha256: "d1321889f262bbbff632e7976349853399cd097b6f382d4b19790c915c13c1ae",
            size_bytes: 292_108,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "black-ops-one",
        display_name: "Black Ops One",
        category: FontCategory::Display,
        tags: &["military", "gaming"],
        license_path: "black-ops-one/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "black-ops-one/BlackOpsOne-Regular.ttf",
            sha256: "282a825b5f294377387e3969f765408157dbea8da0f5d0aae68c6bc704b145b3",
            size_bytes: 166_532,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "russo-one",
        display_name: "Russo One",
        category: FontCategory::Display,
        tags: &["tech", "gaming"],
        license_path: "russo-one/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "russo-one/RussoOne-Regular.ttf",
            sha256: "bc0abcc660bd8b7ad3000ecb2898a27c58a29a50f7ec81652fa12e75148d09df",
            size_bytes: 39_124,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "bangers",
        display_name: "Bangers",
        category: FontCategory::Display,
        tags: &["comic", "callout"],
        license_path: "bangers/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "bangers/Bangers-Regular.ttf",
            sha256: "4160a7311de9342674cce9160cde9fcbb30f48190397d86ff1b70b455af65824",
            size_bytes: 93_148,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "merriweather",
        display_name: "Merriweather",
        category: FontCategory::Serif,
        tags: &["serif", "long-form"],
        license_path: "merriweather/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "merriweather/Merriweather[opsz,wdth,wght].ttf",
            sha256: "d0ed0e359e396af7ad05e73dffd11a3a4c326ea0d0283c56bd9361cb2cc86a96",
            size_bytes: 4_628_080,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "libre-baskerville",
        display_name: "Libre Baskerville",
        category: FontCategory::Serif,
        tags: &["documentary", "editorial"],
        license_path: "libre-baskerville/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "libre-baskerville/LibreBaskerville[wght].ttf",
            sha256: "05a95421961341c5b2556285e8415df9db27dab4f4abe22b446b3c6a8b916c5d",
            size_bytes: 171_900,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "lora",
        display_name: "Lora",
        category: FontCategory::Serif,
        tags: &["serif", "narrative"],
        license_path: "lora/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "lora/Lora[wght].ttf",
            sha256: "822a6621ccbe8d97d20ac88c1c41f5615c9c2c202eaa75f272cd452aac6475a7",
            size_bytes: 212_196,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "cinzel",
        display_name: "Cinzel",
        category: FontCategory::Serif,
        tags: &["cinematic", "classical"],
        license_path: "cinzel/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "cinzel/Cinzel[wght].ttf",
            sha256: "f4d83d34d1f6c741193e4acf4b3dff9531e5a67b6aa65228d00a7db72a4e0f34",
            size_bytes: 125_468,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "bitter",
        display_name: "Bitter",
        category: FontCategory::Serif,
        tags: &["slab-serif", "tutorial"],
        license_path: "bitter/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "bitter/Bitter[wght].ttf",
            sha256: "ef2b9a711fb02f1e5823b34da1b7450e0fc76793b7d733a8b41006e24916d4a7",
            size_bytes: 328_636,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "caveat",
        display_name: "Caveat",
        category: FontCategory::Handwritten,
        tags: &["handwritten", "notes"],
        license_path: "caveat/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "caveat/Caveat[wght].ttf",
            sha256: "0bdb6b660482d31531b3945849fba5916b3ef8695da7024a9e6b9ee3c4157988",
            size_bytes: 403_648,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "pacifico",
        display_name: "Pacifico",
        category: FontCategory::Handwritten,
        tags: &["script", "retro"],
        license_path: "pacifico/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "pacifico/Pacifico-Regular.ttf",
            sha256: "5b6c0d5334a7bf77dea52b975c5a0c408878c0f7115ed5b6fb151f634b7bf701",
            size_bytes: 329_380,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "dancing-script",
        display_name: "Dancing Script",
        category: FontCategory::Handwritten,
        tags: &["script", "friendly"],
        license_path: "dancing-script/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "dancing-script/DancingScript[wght].ttf",
            sha256: "21808625578fe8d8cd10cb684be546dca077b27cd03a53a2f1ec11dc743c924c",
            size_bytes: 133_636,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "comic-neue",
        display_name: "Comic Neue",
        category: FontCategory::Handwritten,
        tags: &["casual", "dialog"],
        license_path: "comic-neue/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[
            FontFace {
                weight: 400,
                source_path: "comic-neue/ComicNeue-Regular.ttf",
                sha256: "a0ee5a37c8b27c4db0700137d928598b1e23b0089e1546a8961909176b779360",
                size_bytes: 57_248,
            },
            FontFace {
                weight: 700,
                source_path: "comic-neue/ComicNeue-Bold.ttf",
                sha256: "3e7e5fccfd7e0788f317b43312151c1bd5cf058c9697a8d83eac3939050bd61e",
                size_bytes: 55_716,
            },
        ],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "gloria-hallelujah",
        display_name: "Gloria Hallelujah",
        category: FontCategory::Handwritten,
        tags: &["marker", "annotation"],
        license_path: "gloria-hallelujah/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[FontFace {
            weight: 400,
            source_path: "gloria-hallelujah/GloriaHallelujah.ttf",
            sha256: "eb59f2762ce8785a292bebb2af3b3e6aa21454913d791f5f25441d1d57ead9fc",
            size_bytes: 59_812,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "jetbrains-mono",
        display_name: "JetBrains Mono",
        category: FontCategory::Monospace,
        tags: &["monospace", "code"],
        license_path: "jetbrains-mono/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "jetbrains-mono/JetBrainsMono[wght].ttf",
            sha256: "48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda",
            size_bytes: 187_208,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "roboto-mono",
        display_name: "Roboto Mono",
        category: FontCategory::Monospace,
        tags: &["monospace", "telemetry"],
        license_path: "roboto-mono/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "roboto-mono/RobotoMono[wght].ttf",
            sha256: "66a80e79d17e4c7cabd162e2916578a4cc08fd19eef6e2a643305eae9c567b2b",
            size_bytes: 183_700,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "space-mono",
        display_name: "Space Mono",
        category: FontCategory::Monospace,
        tags: &["monospace", "retro"],
        license_path: "space-mono/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Static,
        faces: &[
            FontFace {
                weight: 400,
                source_path: "space-mono/SpaceMono-Regular.ttf",
                sha256: "95837e182baeeada83368f7748db28357f0a1b75c6b84ff7065b5edf933c8e18",
                size_bytes: 99_356,
            },
            FontFace {
                weight: 700,
                source_path: "space-mono/SpaceMono-Bold.ttf",
                sha256: "405e73d41afb7e5906efce206a326af5c956f38e255f35421c260e861e599c59",
                size_bytes: 98_232,
            },
        ],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: GF_LATIN_CORE,
    },
    FontFamilyEntry {
        family_id: "noto-sans-arabic",
        display_name: "Noto Sans Arabic",
        category: FontCategory::International,
        tags: &["international", "arabic"],
        license_path: "noto-sans-arabic/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "noto-sans-arabic/NotoSansArabic[wdth,wght].ttf",
            sha256: "63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e",
            size_bytes: 844_676,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: &["gf-latin-core", "international-fallback"],
    },
    FontFamilyEntry {
        family_id: "noto-naskh-arabic",
        display_name: "Noto Naskh Arabic",
        category: FontCategory::International,
        tags: &["international", "arabic"],
        license_path: "noto-naskh-arabic/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "noto-naskh-arabic/NotoNaskhArabic[wght].ttf",
            sha256: "67b5a525a661b607971fbd3f96a81b89d3a768e74534fca84f18ac97e6fab72f",
            size_bytes: 307_592,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: &["gf-latin-core", "international-fallback"],
    },
    FontFamilyEntry {
        family_id: "noto-sans-hebrew",
        display_name: "Noto Sans Hebrew",
        category: FontCategory::International,
        tags: &["international", "hebrew"],
        license_path: "noto-sans-hebrew/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "noto-sans-hebrew/NotoSansHebrew[wdth,wght].ttf",
            sha256: "7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88",
            size_bytes: 112_640,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: &["gf-latin-core", "international-fallback"],
    },
    FontFamilyEntry {
        family_id: "noto-sans-devanagari",
        display_name: "Noto Sans Devanagari",
        category: FontCategory::International,
        tags: &["international", "devanagari"],
        license_path: "noto-sans-devanagari/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "noto-sans-devanagari/NotoSansDevanagari[wdth,wght].ttf",
            sha256: "9ce7b04f60e363d8870e5997744cf85cf69d38a4d7d129d364d92a3b14b461d7",
            size_bytes: 647_144,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: &["gf-latin-core", "international-fallback"],
    },
    FontFamilyEntry {
        family_id: "noto-sans-bengali",
        display_name: "Noto Sans Bengali",
        category: FontCategory::International,
        tags: &["international", "bengali"],
        license_path: "noto-sans-bengali/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "noto-sans-bengali/NotoSansBengali[wdth,wght].ttf",
            sha256: "dcd42978094e584a849c84a51450eeac40c8826057d566ea6d4b9627a403a05a",
            size_bytes: 463_668,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: &["gf-latin-core", "international-fallback"],
    },
    FontFamilyEntry {
        family_id: "noto-sans-tamil",
        display_name: "Noto Sans Tamil",
        category: FontCategory::International,
        tags: &["international", "tamil"],
        license_path: "noto-sans-tamil/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "noto-sans-tamil/NotoSansTamil[wdth,wght].ttf",
            sha256: "aa3a9b321f4b0bb2c40203ffbde9af89713227866e0e13f76e5b9eeea727cf88",
            size_bytes: 340_668,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: &["gf-latin-core", "international-fallback"],
    },
    FontFamilyEntry {
        family_id: "noto-sans-thai",
        display_name: "Noto Sans Thai",
        category: FontCategory::International,
        tags: &["international", "thai"],
        license_path: "noto-sans-thai/OFL.txt",
        license_id: "OFL-1.1",
        source_kind: FontSourceKind::Variable,
        faces: &[FontFace {
            weight: 400,
            source_path: "noto-sans-thai/NotoSansThai[wdth,wght].ttf",
            sha256: "5a1c559bb539583c8a1fd99d1c5b9491e5e14478c9cd2bd0970d5c3096cc9ef8",
            size_bytes: 218_652,
        }],
        default_weight: 400,
        fallback_family_id: Some("lato"),
        glyphset_guarantees: &["gf-latin-core", "international-fallback"],
    },
];

/// Looks up one family by its stable slug.
pub fn find_family(family_id: &str) -> Option<&'static FontFamilyEntry> {
    CATALOG.iter().find(|entry| entry.family_id == family_id)
}

/// Self-consistency checks over [`CATALOG`] alone — no file I/O, so this can run unconditionally
/// (e.g. from a unit test or a future startup smoke check) without touching disk. Bundled-byte
/// verification (hash/size against the real files) is a separate, `include_bytes!`-driven test
/// in this module's own test submodule, since `include_bytes!` needs a literal path per call.
pub fn validate_static_catalog() -> Result<(), Vec<String>> {
    validate_catalog(CATALOG)
}

/// [`validate_static_catalog`] over any list of entries, so the checks themselves can be
/// exercised with deliberately broken data.
fn validate_catalog(catalog: &[FontFamilyEntry]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let mut seen_family_ids = std::collections::HashSet::new();
    let mut seen_source_paths = std::collections::HashSet::new();

    for entry in catalog {
        if entry.family_id.is_empty()
            || !entry
                .family_id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            errors.push(format!(
                "family_id {:?} must be a non-empty lowercase ASCII slug",
                entry.family_id
            ));
        }
        if !seen_family_ids.insert(entry.family_id) {
            errors.push(format!("duplicate family_id {:?}", entry.family_id));
        }
        if entry.faces.is_empty() {
            errors.push(format!("family {:?} declares no faces", entry.family_id));
        }
        if !entry
            .faces
            .iter()
            .any(|face| face.weight == entry.default_weight)
        {
            errors.push(format!(
                "family {:?} default_weight {} has no matching face",
                entry.family_id, entry.default_weight
            ));
        }
        if entry.license_id.is_empty() || entry.license_path.is_empty() {
            errors.push(format!(
                "family {:?} is missing license metadata",
                entry.family_id
            ));
        }
        if let Some(fallback) = entry.fallback_family_id {
            if fallback == entry.family_id {
                errors.push(format!(
                    "family {:?} cannot be its own fallback",
                    entry.family_id
                ));
            } else if !catalog.iter().any(|other| other.family_id == fallback) {
                errors.push(format!(
                    "family {:?} declares unknown fallback_family_id {:?}",
                    entry.family_id, fallback
                ));
            }
        }
        for face in entry.faces {
            if face.sha256.len() != 64 || !face.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                errors.push(format!(
                    "family {:?} face {:?} has a malformed sha256",
                    entry.family_id, face.source_path
                ));
            }
            if !seen_source_paths.insert(face.source_path) {
                errors.push(format!("duplicate source_path {:?}", face.source_path));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

impl TextFontFamily {
    /// This family's stable catalog slug (`"lato"`, `"bebas-neue"`, ...) — the identity FONT-01
    /// intends `.ocproj` to eventually persist instead of the enum variant name. Every
    /// [`TextFontFamily::ALL`] member always resolves to a real [`CATALOG`] entry; a debug build
    /// panics if that invariant is ever broken (guarded by this module's own tests in release
    /// builds too, so the mismatch is caught long before it ships).
    pub fn family_id(&self) -> &str {
        let id: &str = match self {
            // Preserved verbatim -- never a real font_catalog slug, so skip the debug_assert
            // below (which would fire on every unrecognized family otherwise).
            Self::Unknown(id) => return id.as_str(),
            Self::Lato => "lato",
            Self::BebasNeue => "bebas-neue",
            Self::PlayfairDisplay => "playfair-display-sc",
            Self::PatrickHand => "patrick-hand",
            Self::AnonymousPro => "anonymous-pro",
            Self::ArchivoBlack => "archivo-black",
            Self::Inter => "inter",
            Self::Montserrat => "montserrat",
            Self::Roboto => "roboto",
            Self::OpenSans => "open-sans",
            Self::Poppins => "poppins",
            Self::Nunito => "nunito",
            Self::SourceSans3 => "source-sans-3",
            Self::Barlow => "barlow",
            Self::Fredoka => "fredoka",
            Self::Oswald => "oswald",
            Self::Anton => "anton",
            Self::BarlowCondensed => "barlow-condensed",
            Self::LeagueSpartan => "league-spartan",
            Self::Teko => "teko",
            Self::BlackOpsOne => "black-ops-one",
            Self::RussoOne => "russo-one",
            Self::Bangers => "bangers",
            Self::Merriweather => "merriweather",
            Self::LibreBaskerville => "libre-baskerville",
            Self::Lora => "lora",
            Self::Cinzel => "cinzel",
            Self::Bitter => "bitter",
            Self::Caveat => "caveat",
            Self::Pacifico => "pacifico",
            Self::DancingScript => "dancing-script",
            Self::ComicNeue => "comic-neue",
            Self::GloriaHallelujah => "gloria-hallelujah",
            Self::JetBrainsMono => "jetbrains-mono",
            Self::RobotoMono => "roboto-mono",
            Self::SpaceMono => "space-mono",
            Self::NotoSansArabic => "noto-sans-arabic",
            Self::NotoNaskhArabic => "noto-naskh-arabic",
            Self::NotoSansHebrew => "noto-sans-hebrew",
            Self::NotoSansDevanagari => "noto-sans-devanagari",
            Self::NotoSansBengali => "noto-sans-bengali",
            Self::NotoSansTamil => "noto-sans-tamil",
            Self::NotoSansThai => "noto-sans-thai",
        };
        debug_assert!(
            find_family(id).is_some(),
            "TextFontFamily variant has no matching font_catalog entry: {id}"
        );
        id
    }

    /// Reverse of [`Self::family_id`]. Returns `None` for a slug this build doesn't recognize —
    /// per the doc's forwards-compatibility rule, callers must keep the raw id and fall back to
    /// [`Self::default`] for rendering rather than treating an unknown id as corruption.
    pub fn from_family_id(family_id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|family| family.family_id() == family_id)
    }
}

/// One locked face's bytes, keyed by `(family_id, weight)` — the single `include_bytes!` source
/// of truth for every locked binary. [`crate::text_metrics`]'s `fontdue`-based table and
/// [`crate::text_layout`]'s `cosmic-text`-based one (TEXT-01A) both load from here rather than
/// each keeping their own `include_bytes!` list, so the two engines can never silently diverge on
/// which bytes a family/weight resolves to.
pub fn locked_face_bytes() -> &'static [(&'static str, u16, &'static [u8])] {
    &[
        (
            "lato",
            400,
            include_bytes!("../assets/fonts/lato/Lato-Regular.ttf"),
        ),
        (
            "lato",
            700,
            include_bytes!("../assets/fonts/lato/Lato-Bold.ttf"),
        ),
        (
            "bebas-neue",
            400,
            include_bytes!("../assets/fonts/bebas-neue/BebasNeue-Regular.ttf"),
        ),
        (
            "playfair-display-sc",
            400,
            include_bytes!("../assets/fonts/playfair-display-sc/PlayfairDisplaySC-Regular.ttf"),
        ),
        (
            "playfair-display-sc",
            700,
            include_bytes!("../assets/fonts/playfair-display-sc/PlayfairDisplaySC-Bold.ttf"),
        ),
        (
            "patrick-hand",
            400,
            include_bytes!("../assets/fonts/patrick-hand/PatrickHand-Regular.ttf"),
        ),
        (
            "anonymous-pro",
            400,
            include_bytes!("../assets/fonts/anonymous-pro/AnonymousPro-Regular.ttf"),
        ),
        (
            "anonymous-pro",
            700,
            include_bytes!("../assets/fonts/anonymous-pro/AnonymousPro-Bold.ttf"),
        ),
        (
            "archivo-black",
            400,
            include_bytes!("../assets/fonts/archivo-black/ArchivoBlack-Regular.ttf"),
        ),
        (
            "inter",
            400,
            include_bytes!("../assets/fonts/inter/Inter[opsz,wght].ttf"),
        ),
        (
            "montserrat",
            400,
            include_bytes!("../assets/fonts/montserrat/Montserrat[wght].ttf"),
        ),
        (
            "roboto",
            400,
            include_bytes!("../assets/fonts/roboto/Roboto[wdth,wght].ttf"),
        ),
        (
            "open-sans",
            400,
            include_bytes!("../assets/fonts/open-sans/OpenSans[wdth,wght].ttf"),
        ),
        (
            "poppins",
            400,
            include_bytes!("../assets/fonts/poppins/Poppins-Regular.ttf"),
        ),
        (
            "poppins",
            700,
            include_bytes!("../assets/fonts/poppins/Poppins-Bold.ttf"),
        ),
        (
            "nunito",
            400,
            include_bytes!("../assets/fonts/nunito/Nunito[wght].ttf"),
        ),
        (
            "source-sans-3",
            400,
            include_bytes!("../assets/fonts/source-sans-3/SourceSans3[wght].ttf"),
        ),
        (
            "barlow",
            400,
            include_bytes!("../assets/fonts/barlow/Barlow-Regular.ttf"),
        ),
        (
            "barlow",
            700,
            include_bytes!("../assets/fonts/barlow/Barlow-Bold.ttf"),
        ),
        (
            "fredoka",
            400,
            include_bytes!("../assets/fonts/fredoka/Fredoka[wdth,wght].ttf"),
        ),
        (
            "oswald",
            400,
            include_bytes!("../assets/fonts/oswald/Oswald[wght].ttf"),
        ),
        (
            "anton",
            400,
            include_bytes!("../assets/fonts/anton/Anton-Regular.ttf"),
        ),
        (
            "barlow-condensed",
            400,
            include_bytes!("../assets/fonts/barlow-condensed/BarlowCondensed-Regular.ttf"),
        ),
        (
            "barlow-condensed",
            700,
            include_bytes!("../assets/fonts/barlow-condensed/BarlowCondensed-Bold.ttf"),
        ),
        (
            "league-spartan",
            400,
            include_bytes!("../assets/fonts/league-spartan/LeagueSpartan[wght].ttf"),
        ),
        (
            "teko",
            400,
            include_bytes!("../assets/fonts/teko/Teko[wght].ttf"),
        ),
        (
            "black-ops-one",
            400,
            include_bytes!("../assets/fonts/black-ops-one/BlackOpsOne-Regular.ttf"),
        ),
        (
            "russo-one",
            400,
            include_bytes!("../assets/fonts/russo-one/RussoOne-Regular.ttf"),
        ),
        (
            "bangers",
            400,
            include_bytes!("../assets/fonts/bangers/Bangers-Regular.ttf"),
        ),
        (
            "merriweather",
            400,
            include_bytes!("../assets/fonts/merriweather/Merriweather[opsz,wdth,wght].ttf"),
        ),
        (
            "libre-baskerville",
            400,
            include_bytes!("../assets/fonts/libre-baskerville/LibreBaskerville[wght].ttf"),
        ),
        (
            "lora",
            400,
            include_bytes!("../assets/fonts/lora/Lora[wght].ttf"),
        ),
        (
            "cinzel",
            400,
            include_bytes!("../assets/fonts/cinzel/Cinzel[wght].ttf"),
        ),
        (
            "bitter",
            400,
            include_bytes!("../assets/fonts/bitter/Bitter[wght].ttf"),
        ),
        (
            "caveat",
            400,
            include_bytes!("../assets/fonts/caveat/Caveat[wght].ttf"),
        ),
        (
            "pacifico",
            400,
            include_bytes!("../assets/fonts/pacifico/Pacifico-Regular.ttf"),
        ),
        (
            "dancing-script",
            400,
            include_bytes!("../assets/fonts/dancing-script/DancingScript[wght].ttf"),
        ),
        (
            "comic-neue",
            400,
            include_bytes!("../assets/fonts/comic-neue/ComicNeue-Regular.ttf"),
        ),
        (
            "comic-neue",
            700,
            include_bytes!("../assets/fonts/comic-neue/ComicNeue-Bold.ttf"),
        ),
        (
            "gloria-hallelujah",
            400,
            include_bytes!("../assets/fonts/gloria-hallelujah/GloriaHallelujah.ttf"),
        ),
        (
            "jetbrains-mono",
            400,
            include_bytes!("../assets/fonts/jetbrains-mono/JetBrainsMono[wght].ttf"),
        ),
        (
            "roboto-mono",
            400,
            include_bytes!("../assets/fonts/roboto-mono/RobotoMono[wght].ttf"),
        ),
        (
            "space-mono",
            400,
            include_bytes!("../assets/fonts/space-mono/SpaceMono-Regular.ttf"),
        ),
        (
            "space-mono",
            700,
            include_bytes!("../assets/fonts/space-mono/SpaceMono-Bold.ttf"),
        ),
        (
            "noto-sans-arabic",
            400,
            include_bytes!("../assets/fonts/noto-sans-arabic/NotoSansArabic[wdth,wght].ttf"),
        ),
        (
            "noto-naskh-arabic",
            400,
            include_bytes!("../assets/fonts/noto-naskh-arabic/NotoNaskhArabic[wght].ttf"),
        ),
        (
            "noto-sans-hebrew",
            400,
            include_bytes!("../assets/fonts/noto-sans-hebrew/NotoSansHebrew[wdth,wght].ttf"),
        ),
        (
            "noto-sans-devanagari",
            400,
            include_bytes!(
                "../assets/fonts/noto-sans-devanagari/NotoSansDevanagari[wdth,wght].ttf"
            ),
        ),
        (
            "noto-sans-bengali",
            400,
            include_bytes!("../assets/fonts/noto-sans-bengali/NotoSansBengali[wdth,wght].ttf"),
        ),
        (
            "noto-sans-tamil",
            400,
            include_bytes!("../assets/fonts/noto-sans-tamil/NotoSansTamil[wdth,wght].ttf"),
        ),
        (
            "noto-sans-thai",
            400,
            include_bytes!("../assets/fonts/noto-sans-thai/NotoSansThai[wdth,wght].ttf"),
        ),
    ]
}

/// Looks up one locked face's bytes by `family_id`/`weight`.
pub fn face_bytes(family_id: &str, weight: u16) -> Option<&'static [u8]> {
    locked_face_bytes()
        .iter()
        .find(|(id, w, _)| *id == family_id && *w == weight)
        .map(|(_, _, bytes)| *bytes)
}

#[cfg(test)]
#[path = "font_catalog/font_catalog_test.rs"]
mod tests;
