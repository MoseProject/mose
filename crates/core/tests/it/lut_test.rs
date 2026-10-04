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

use avcore::{Lut3D, LutParseError};

const IDENTITY_2: &str = "TITLE \"identity\"\nLUT_3D_SIZE 2\n\
0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

#[test]
fn a_cube_file_parses_into_its_size_and_entries() {
    let lut = Lut3D::parse(IDENTITY_2).unwrap();
    assert_eq!(lut.size(), 2);
    assert_eq!(lut.entries().len(), 8);
    assert_eq!(lut.entries()[1], [1.0, 0.0, 0.0]);
    let corner = lut.sample(1.0, 1.0, 1.0);
    assert!(corner.iter().all(|c| (c - 1.0).abs() < 1e-5));
}

#[test]
fn comments_blank_lines_and_domain_metadata_are_ignored() {
    let text = format!(
        "# made by a test\n\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\nLUT_1D_SIZE 4\n{IDENTITY_2}"
    );
    assert_eq!(Lut3D::parse(&text).unwrap().size(), 2);
}

#[test]
fn every_malformed_cube_has_its_own_error_and_message() {
    let cases: [(&str, LutParseError, &str); 5] = [
        (
            "0 0 0\n",
            LutParseError::MissingSize,
            "no LUT_3D_SIZE line found",
        ),
        (
            "LUT_3D_SIZE two\n",
            LutParseError::InvalidSize,
            "LUT_3D_SIZE isn't a valid integer",
        ),
        (
            "LUT_3D_SIZE 1\n",
            LutParseError::SizeTooSmall(1),
            "LUT_3D_SIZE must be at least 2, got 1",
        ),
        (
            "LUT_3D_SIZE 2\n0 0 0\n",
            LutParseError::WrongRowCount {
                expected: 8,
                found: 1,
            },
            "expected 8 data rows, found 1",
        ),
        (
            "LUT_3D_SIZE 2\n0 0\n",
            LutParseError::MalformedRow(0),
            "data row 0 doesn't have 3 numbers",
        ),
    ];
    for (text, error, message) in cases {
        let got = Lut3D::parse(text).unwrap_err();
        assert_eq!(got, error, "{text:?}");
        assert_eq!(got.to_string(), message);
    }
    for text in [
        "LUT_3D_SIZE 2\nred green blue\n",
        "LUT_3D_SIZE 2\n0 0 NaN\n",
    ] {
        assert_eq!(
            Lut3D::parse(text).unwrap_err(),
            LutParseError::MalformedRow(0)
        );
    }
}

#[test]
fn a_cube_file_loads_from_disk_and_a_missing_one_reports_the_size_error() {
    let path = std::env::temp_dir().join(format!("oca_lut_test_{}.cube", std::process::id()));
    std::fs::write(&path, IDENTITY_2).unwrap();
    let lut = Lut3D::load(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(lut.size(), 2);
    assert_eq!(
        Lut3D::load(&path).unwrap_err(),
        LutParseError::MissingSize,
        "an unreadable file reports the same error as a file with no size"
    );
}
