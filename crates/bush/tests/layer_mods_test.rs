use bush::{Lut3D, LutError};

#[test]
fn a_lut_must_have_a_supported_size() {
    assert_eq!(
        Lut3D::new(1, vec![[0.0; 3]]).unwrap_err(),
        LutError::InvalidSize(1)
    );
    assert_eq!(
        Lut3D::new(66, Vec::new()).unwrap_err(),
        LutError::InvalidSize(66)
    );
    let lut = Lut3D::new(2, vec![[0.5; 3]; 8]).unwrap();
    assert_eq!(lut.size(), 2);
}
