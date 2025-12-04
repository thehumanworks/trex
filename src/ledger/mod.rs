pub mod account;
pub mod engine;
pub mod transaction;

pub fn serialize_4dp<S: serde::Serializer>(val: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&format!("{:.4}", val))
}

pub fn serialize_4dp_or_none<S: serde::Serializer>(
    val: &Option<f64>,
    s: S,
) -> Result<S::Ok, S::Error> {
    if let Some(val) = val {
        s.serialize_str(&format!("{:.4}", val))
    } else {
        s.serialize_none()
    }
}
