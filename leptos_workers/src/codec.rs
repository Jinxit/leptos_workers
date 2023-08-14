#[cfg(feature = "json")]
pub(crate) use serde_json::from_slice;

#[cfg(feature = "json")]
pub(crate) use serde_json::to_vec;

/*
#[cfg(feature = "bincode")]
use serde::de::DeserializeOwned;

#[cfg(feature = "bincode")]
pub(crate) fn from_slice<T>(v: &[u8]) -> Result<T, bincode::error::DecodeError>
where
    T: DeserializeOwned,
{
    bincode::serde::decode_from_slice(v, bincode::config::standard()).map(|(result, _)| result)
}

#[cfg(feature = "bincode")]
pub(crate) fn to_vec<T>(value: &T) -> Result<Vec<u8>, bincode::error::EncodeError>
where
    T: ?Sized + serde::ser::Serialize,
{
    bincode::serde::encode_to_vec(value, bincode::config::standard())
}
*/
